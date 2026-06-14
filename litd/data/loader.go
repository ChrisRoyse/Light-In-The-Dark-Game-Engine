// Package data is the WC3 SLK/object-data analogue: plain tables in
// data/, loaded once at startup, immutable at runtime
// (validation-and-data.md §3, R-AST-1).
//
// TOML is the authored format; JSON is accepted for generated tables.
// Both decode into ONE in-memory schema, with every gameplay value
// converted at load — seconds to integer ticks (50 ms grid, ceil),
// world-units-per-second speeds to per-tick fixed.F64, revolutions
// per second to BAM per tick, type names to matrix indices. No float
// survives into the schema, so nothing downstream can drift (R-SIM-1).
//
// The loader fails closed: unknown fields, out-of-range values, and
// dangling ability references are LOAD errors, never runtime
// fallbacks. The canonical fingerprint hashes the CONVERTED schema in
// fixed field order — independent of key order and source format,
// sensitive to every value — and folds into the state-hash preamble
// and replay header (R-SIM-2).
package data

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io/fs"
	"math"
	"path"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// TickMS is the sim tick in milliseconds (R-EXEC-5).
const TickMS = 50

// TicksPerSecond is the integer tick rate.
const TicksPerSecond = 1000 / TickMS

// FormatVersion is the data-format version, folded into the
// fingerprint; bump on schema change.
const FormatVersion = 2

// Pathing classes.
const (
	PathingGround uint8 = iota
	PathingAir
)

// Unit races (GetUnitRace). Values mirror the WC3 race constants so the
// ConvertRace integer mapping round-trips: HUMAN=1..DEMON=5, OTHER=7.
// RaceNone (0) is the unset/unconfigured default.
const (
	RaceNone     uint8 = 0
	RaceHuman    uint8 = 1
	RaceOrc      uint8 = 2
	RaceUndead   uint8 = 3
	RaceNightElf uint8 = 4
	RaceDemon    uint8 = 5
	RaceOther    uint8 = 7
)

// Attack delivery (combat-and-orders.md §3.4).
const (
	DeliveryInstant uint8 = iota
	DeliveryProjectile
)

// TargetsAllowed bits.
const (
	TargetGround uint16 = 1 << iota
	TargetAir
	TargetStructure
)

// Tables is the loaded, immutable data set.
type Tables struct {
	AttackTypes   []string           // damage-matrix row names, table order
	ArmorTypes    []string           // damage-matrix column names, table order
	Coeff         [][]int32          // [attackType][armorType] per-mille coefficient
	BuffTypes     []BuffType         // sorted by ID (#162)
	Abilities     []Ability          // sorted by ID
	Units         []Unit             // sorted by ID
	Effects       []CompiledEffect   // flat effect-composition arena (ADR #294)
	Smart         *SmartTable        // smart-order resolution; nil when orders/ absent
	ResourceTypes []string           // economy resource registry, table order (#300)
	Nodes         []ResourceNodeType // resource-node types, sorted by ID
	Upgrades      []Upgrade          // tech upgrades, sorted by ID (#303)
	Requires      []Require          // admission requirements, sorted (#303)
	Hero          *HeroTables        // hero rule set; nil when heroes/ absent (#304)
	Items         []Item             // item types, sorted by ID (#305)
	Fingerprint   uint64             // canonical content hash (state-hash preamble)
}

// Ability is one ability row. Behavior is the compiled effect
// composition (#296); cast mechanics are sim-unit converted at load
// (#160). A zero-valued cast block is a passive/stub row.
type Ability struct {
	ID             string
	Name           string
	Effects        EffectList // zero-length when the row declares none
	ManaCost       int32
	CooldownTicks  uint16
	CastPointTicks uint16
	BackswingTicks uint16
	ChannelTicks   uint16
	CastRange      fixed.F64 // 0 = self/no-target cast
}

// Unit is one converted unit row. Every field is in sim units:
// ticks, per-tick fixed-point, BAM, matrix indices.
type Unit struct {
	ID               string
	Life             int32
	RegenPerTick     fixed.F64 // hit points per tick
	Armor            int32
	ArmorType        uint8 // index into ArmorTypes
	MoveSpeedPerTick fixed.F64
	TurnRatePerTick  fixed.Angle // BAM per tick
	CollisionSize    int32       // world-unit radius
	CollisionClass   uint8       // dilation margin in cells (path.CellRadius)
	Pathing          uint8
	AcquisitionRange fixed.F64
	SightDay         fixed.F64
	SightNight       fixed.F64
	FlyHeight        fixed.F64 // default flight height in world units (#367)
	Model            string
	Name             string // display/proper name (GetUnitName); "" = unnamed
	PointValue       int32  // score/bounty weight (GetUnitPointValue); 0 = none
	Level            int32  // design level (GetUnitLevel for non-heroes); 0 = unset
	Race             uint8  // Race* (GetUnitRace); RaceNone = unset
	Attacks          []Attack
	Abilities        []uint16 // indices into Tables.Abilities

	// economy block (#300)
	FoodCost     uint8
	FoodProvided uint8
	DepotMask    uint16      // bit per accepted resource index
	Harvest      HarvestSpec // Capacity 0 = not a harvester

	// production block (#302)
	Costs      []int64  // per resource index (len = registry size; nil = free)
	TrainTicks uint16   // 0 = not trainable
	Trains     []uint16 // unit indices this building can train (post-sort resolve)
	Researches []uint16 // upgrade indices this building can research (#303)

	RevivesHeroes bool // altar-class building (#304)

	// construction block (#301): a unit with BuildTicks > 0 is a
	// worker-constructable building. Footprint is the square side in
	// grid cells stamped onto the pathing grid; Costs (above) is the
	// build cost; Life is the finished max HP. RefundPermille is the
	// per-mille of cost returned on cancel.
	Footprint      uint8  // square side in cells; 0 = not a building
	BuildTicks     uint16 // construction duration; 0 = not constructable
	RefundPermille uint16 // cancel refund fraction (0..1000)
}

// Attack is one converted weapon row.
type Attack struct {
	AttackType             uint8 // index into AttackTypes
	Range                  fixed.F64
	DamageBase             int32
	Dice                   int32
	Sides                  int32
	CooldownTicks          uint16
	DamagePointTicks       uint16
	BackswingTicks         uint16
	Delivery               uint8
	ProjectileSpeedPerTick fixed.F64 // zero for instant
	TargetsAllowed         uint16
}

// ---- raw (decode-time) shapes; floats live ONLY here ----

type rawUnitFile struct {
	Unit []rawUnit `toml:"unit" json:"unit"`
}

type rawUnit struct {
	ID               string           `toml:"id" json:"id"`
	Life             int64            `toml:"life" json:"life"`
	Regen            float64          `toml:"regen" json:"regen"`
	Armor            int64            `toml:"armor" json:"armor"`
	ArmorType        string           `toml:"armor-type" json:"armor-type"`
	MoveSpeed        float64          `toml:"move-speed" json:"move-speed"`
	TurnRate         float64          `toml:"turn-rate" json:"turn-rate"`
	CollisionSize    int64            `toml:"collision-size" json:"collision-size"`
	Pathing          string           `toml:"pathing" json:"pathing"`
	AcquisitionRange float64          `toml:"acquisition-range" json:"acquisition-range"`
	SightDay         float64          `toml:"sight-day" json:"sight-day"`
	SightNight       float64          `toml:"sight-night" json:"sight-night"`
	FlyHeight        float64          `toml:"fly-height" json:"fly-height"`
	Model            string           `toml:"model" json:"model"`
	Name             string           `toml:"name" json:"name"`
	PointValue       int64            `toml:"point-value" json:"point-value"`
	Level            int64            `toml:"level" json:"level"`
	Race             string           `toml:"race" json:"race"`
	Abilities        []string         `toml:"abilities" json:"abilities"`
	Attacks          []rawAttack      `toml:"attack" json:"attack"`
	FoodCost         int64            `toml:"food-cost" json:"food-cost"`
	FoodProvided     int64            `toml:"food-provided" json:"food-provided"`
	DepotFor         []string         `toml:"depot-for" json:"depot-for"`
	Harvest          *rawHarvest      `toml:"harvest" json:"harvest"`
	Costs            map[string]int64 `toml:"costs" json:"costs"`
	TrainSeconds     float64          `toml:"train-seconds" json:"train-seconds"`
	Trains           []string         `toml:"trains" json:"trains"`
	Researches       []string         `toml:"researches" json:"researches"`
	RevivesHeroes    bool             `toml:"revives-heroes" json:"revives-heroes"`
	Footprint        int64            `toml:"footprint" json:"footprint"`
	BuildSeconds     float64          `toml:"build-seconds" json:"build-seconds"`
	RefundPermille   int64            `toml:"refund-permille" json:"refund-permille"`
}

type rawAttack struct {
	Type            string   `toml:"type" json:"type"`
	Range           float64  `toml:"range" json:"range"`
	DamageBase      int64    `toml:"damage-base" json:"damage-base"`
	Dice            int64    `toml:"dice" json:"dice"`
	Sides           int64    `toml:"sides" json:"sides"`
	Cooldown        float64  `toml:"cooldown" json:"cooldown"`
	DamagePoint     float64  `toml:"damage-point" json:"damage-point"`
	Backswing       float64  `toml:"backswing" json:"backswing"`
	Delivery        string   `toml:"delivery" json:"delivery"`
	ProjectileSpeed float64  `toml:"projectile-speed" json:"projectile-speed"`
	TargetsAllowed  []string `toml:"targets-allowed" json:"targets-allowed"`
}

type rawDamageTable struct {
	AttackTypes  []string           `toml:"attack-types" json:"attack-types"`
	ArmorTypes   []string           `toml:"armor-types" json:"armor-types"`
	Coefficients map[string][]int64 `toml:"coefficients" json:"coefficients"`
}

type rawAbilityFile struct {
	Ability []rawAbility `toml:"ability" json:"ability"`
}

type rawAbility struct {
	ID        string  `toml:"id" json:"id"`
	Name      string  `toml:"name" json:"name"`
	ManaCost  int64   `toml:"mana-cost" json:"mana-cost"`
	Cooldown  float64 `toml:"cooldown" json:"cooldown"`     // seconds
	CastPoint float64 `toml:"cast-point" json:"cast-point"` // seconds
	Backswing float64 `toml:"backswing" json:"backswing"`   // seconds
	Channel   float64 `toml:"channel" json:"channel"`       // seconds
	CastRange float64 `toml:"cast-range" json:"cast-range"` // world units
	// Effects is the raw composition tree; the effect compiler owns
	// all validation inside the maps (decodeStrict cannot see them).
	Effects []map[string]any `toml:"effects" json:"effects"`
}

// ---- strict decoding ----

// decodeStrict decodes TOML or JSON (by extension) and rejects any
// field the schema does not declare — naming the field and the file.
// opaque lists key prefixes whose subtrees decode into any-typed
// containers (the TOML metadata cannot see inside them); the owning
// validator — the effect compiler — enforces strictness there
// instead, so nothing is exempt from SOME strict check.
func decodeStrict(file string, blob []byte, v any, opaque ...string) error {
	switch path.Ext(file) {
	case ".toml":
		md, err := toml.Decode(string(blob), v)
		if err != nil {
			return fmt.Errorf("data: %s: %w", file, err)
		}
	undecoded:
		for _, un := range md.Undecoded() {
			k := un.String()
			for _, p := range opaque {
				if strings.HasPrefix(k, p+".") {
					continue undecoded
				}
			}
			return fmt.Errorf("data: %s: unknown field %q (schema rejects unrecognized keys)", file, k)
		}
		return nil
	case ".json":
		dec := json.NewDecoder(bytes.NewReader(blob))
		dec.DisallowUnknownFields()
		if err := dec.Decode(v); err != nil {
			return fmt.Errorf("data: %s: %w", file, err)
		}
		return nil
	default:
		return fmt.Errorf("data: %s: unsupported table format %q", file, path.Ext(file))
	}
}

// ---- conversions (load-time only; results are integers) ----

// SecondsToTicks quantizes a duration onto the 50 ms tick grid,
// rounding UP — a sub-tick value becomes 1 tick, never 0, so authored
// timings are never faster than written.
func SecondsToTicks(s float64) (uint16, error) {
	if s < 0 || s > 3000 || math.IsNaN(s) {
		return 0, fmt.Errorf("duration %v s out of range [0, 3000]", s)
	}
	ms := int64(math.Round(s * 1000))
	if ms == 0 {
		return 0, nil
	}
	t := (ms + TickMS - 1) / TickMS
	if t > math.MaxUint16 {
		return 0, fmt.Errorf("duration %v s overflows tick counter", s)
	}
	return uint16(t), nil
}

// perSecondToPerTick converts a world-units-per-second rate to the
// per-tick fixed-point increment.
func perSecondToPerTick(v float64) (fixed.F64, error) {
	if v < 0 || v > 100000 || math.IsNaN(v) {
		return 0, fmt.Errorf("rate %v out of range [0, 100000]", v)
	}
	return fixed.F64(int64(math.Round(v * float64(fixed.One) / TicksPerSecond))), nil
}

// turnRateToBAM converts revolutions-per-second to BAM per tick.
func turnRateToBAM(v float64) (fixed.Angle, error) {
	if v < 0 || v > 10 || math.IsNaN(v) {
		return 0, fmt.Errorf("turn-rate %v out of range [0, 10]", v)
	}
	return fixed.Angle(uint16(math.Round(v * 65536 / TicksPerSecond))), nil
}

// worldUnits converts a distance to fixed-point world units.
func worldUnits(v float64) (fixed.F64, error) {
	if v < 0 || v > 1<<20 || math.IsNaN(v) {
		return 0, fmt.Errorf("distance %v out of range [0, 2^20]", v)
	}
	return fixed.F64(int64(math.Round(v * float64(fixed.One)))), nil
}

// cellRadius mirrors path.CellRadius (cell = 32 world units, boundary
// touch is conservative) without importing litd/sim/path — data must
// stay leaf-level.
func cellRadius(worldRadius int32) uint8 {
	return uint8((worldRadius + 16) / 32)
}

func indexOf(list []string, s string) int {
	for i := range list {
		if list[i] == s {
			return i
		}
	}
	return -1
}

// ---- Load ----

// Load reads the full data layout from fsys (the data/ directory):
// combat/damage-table.{toml|json}, abilities/*.{toml|json},
// units/*.{toml|json}. File order is sorted — deterministic.
func Load(fsys fs.FS) (*Tables, error) {
	t := &Tables{}

	// damage matrix first: the type vocabularies everything else
	// validates against
	dtFile, dtBlob, err := readOne(fsys, "combat", "damage-table")
	if err != nil {
		return nil, err
	}
	var rawDT rawDamageTable
	if err := decodeStrict(dtFile, dtBlob, &rawDT); err != nil {
		return nil, err
	}
	if err := t.installDamageTable(dtFile, &rawDT); err != nil {
		return nil, err
	}

	// buffs before abilities: apply-buff params resolve buff names at
	// compile time (#162). One compiler — buff periodic compositions
	// and ability compositions share the arena.
	comp := &effectCompiler{attackTypes: t.AttackTypes}
	if err := t.loadBuffs(fsys, comp); err != nil {
		return nil, err
	}

	// abilities (the reference set). Raw rows are collected with
	// their source file, sorted by ID, THEN compiled — the effect
	// arena layout depends only on the ID order, never on file
	// enumeration.
	abilityFiles, err := listTables(fsys, "abilities")
	if err != nil {
		return nil, err
	}
	type pendingAbility struct {
		file string
		raw  rawAbility
	}
	var pending []pendingAbility
	for _, f := range abilityFiles {
		blob, err := fs.ReadFile(fsys, f)
		if err != nil {
			return nil, fmt.Errorf("data: %s: %w", f, err)
		}
		var raw rawAbilityFile
		if err := decodeStrict(f, blob, &raw, "ability.effects"); err != nil {
			return nil, err
		}
		for i := range raw.Ability {
			if raw.Ability[i].ID == "" {
				return nil, fmt.Errorf("data: %s: ability with empty id", f)
			}
			pending = append(pending, pendingAbility{file: f, raw: raw.Ability[i]})
		}
	}
	sort.Slice(pending, func(i, j int) bool { return pending[i].raw.ID < pending[j].raw.ID })
	for i := 1; i < len(pending); i++ {
		if pending[i].raw.ID == pending[i-1].raw.ID {
			return nil, fmt.Errorf("data: duplicate ability id %q (%s, %s)",
				pending[i].raw.ID, pending[i-1].file, pending[i].file)
		}
	}
	for _, p := range pending {
		ab, err := convertAbilityCast(p.file, &p.raw)
		if err != nil {
			return nil, err
		}
		if p.raw.Effects != nil {
			comp.file = p.file
			where := fmt.Sprintf("ability %q effects", p.raw.ID)
			lst, inv, err := comp.compile(where, p.raw.Effects, 1)
			if err != nil {
				return nil, err
			}
			if inv > MaxEffectInvocations {
				return nil, fmt.Errorf("data: %s: %s: worst-case invocation count %d exceeds ceiling %d",
					p.file, where, inv, MaxEffectInvocations)
			}
			ab.Effects = lst
		}
		t.Abilities = append(t.Abilities, ab)
	}
	// the arena seals AFTER items: their use pipelines share it (#305)

	// economy registry before units: unit econ blocks validate
	// against the resource-type vocabulary (#300)
	if err := t.loadEconomy(fsys); err != nil {
		return nil, err
	}

	// units
	unitFiles, err := listTables(fsys, "units")
	if err != nil {
		return nil, err
	}
	pendingTrains := map[string][]string{}
	pendingResearches := map[string][]string{}
	for _, f := range unitFiles {
		blob, err := fs.ReadFile(fsys, f)
		if err != nil {
			return nil, fmt.Errorf("data: %s: %w", f, err)
		}
		var raw rawUnitFile
		if err := decodeStrict(f, blob, &raw); err != nil {
			return nil, err
		}
		for i := range raw.Unit {
			u, err := t.convertUnit(f, &raw.Unit[i])
			if err != nil {
				return nil, err
			}
			if len(raw.Unit[i].Trains) > 0 {
				pendingTrains[u.ID] = raw.Unit[i].Trains
			}
			if len(raw.Unit[i].Researches) > 0 {
				pendingResearches[u.ID] = raw.Unit[i].Researches
			}
			t.Units = append(t.Units, u)
		}
	}
	sort.Slice(t.Units, func(i, j int) bool { return t.Units[i].ID < t.Units[j].ID })
	for i := 1; i < len(t.Units); i++ {
		if t.Units[i].ID == t.Units[i-1].ID {
			return nil, fmt.Errorf("data: duplicate unit id %q", t.Units[i].ID)
		}
	}
	// trains lists resolve AFTER the sort: unit indices are stable
	// only once the canonical order exists (#302)
	for i := range t.Units {
		for _, ref := range pendingTrains[t.Units[i].ID] {
			idx := sort.Search(len(t.Units), func(k int) bool { return t.Units[k].ID >= ref })
			if idx == len(t.Units) || t.Units[idx].ID != ref {
				return nil, fmt.Errorf("data: unit %q: trains reference to undefined unit %q", t.Units[i].ID, ref)
			}
			if t.Units[idx].TrainTicks == 0 {
				return nil, fmt.Errorf("data: unit %q: trains %q which has no train-seconds", t.Units[i].ID, ref)
			}
			t.Units[i].Trains = append(t.Units[i].Trains, uint16(idx))
		}
	}

	// tech tree after the unit sort: applies-to / requirement / unit
	// researches references need stable unit indices (#303)
	if err := t.loadTech(fsys, pendingResearches); err != nil {
		return nil, err
	}

	// hero tables after units + abilities (#304)
	if err := t.loadHeroes(fsys); err != nil {
		return nil, err
	}

	// items after economy (costs) + buffs (mod vocabulary); they
	// compile into the shared effect arena, which seals here (#305)
	if err := t.loadItems(fsys, comp); err != nil {
		return nil, err
	}
	t.Effects = comp.arena

	// smart-order table (optional directory; absence is visible as nil,
	// never silently defaulted)
	if files, _ := listTables(fsys, "orders"); len(files) > 0 {
		smart, err := LoadSmart(fsys)
		if err != nil {
			return nil, err
		}
		t.Smart = smart
	}

	t.Fingerprint = t.fingerprint()
	return t, nil
}

// readOne finds dir/base.toml or dir/base.json — exactly one.
func readOne(fsys fs.FS, dir, base string) (string, []byte, error) {
	var found []string
	for _, ext := range []string{".toml", ".json"} {
		f := path.Join(dir, base+ext)
		if _, err := fs.Stat(fsys, f); err == nil {
			found = append(found, f)
		}
	}
	if len(found) != 1 {
		return "", nil, fmt.Errorf("data: want exactly one %s/%s.{toml|json}, found %d", dir, base, len(found))
	}
	blob, err := fs.ReadFile(fsys, found[0])
	if err != nil {
		return "", nil, fmt.Errorf("data: %s: %w", found[0], err)
	}
	return found[0], blob, nil
}

// listTables returns the sorted .toml/.json files of a directory.
// A missing directory is an empty table set, not an error — absence
// is visible (zero rows), not masked.
func listTables(fsys fs.FS, dir string) ([]string, error) {
	entries, err := fs.ReadDir(fsys, dir)
	if err != nil {
		return nil, nil
	}
	var out []string
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		ext := path.Ext(e.Name())
		if ext == ".toml" || ext == ".json" {
			out = append(out, path.Join(dir, e.Name()))
		}
	}
	sort.Strings(out)
	return out, nil
}

func (t *Tables) installDamageTable(file string, raw *rawDamageTable) error {
	if len(raw.AttackTypes) == 0 || len(raw.ArmorTypes) == 0 {
		return fmt.Errorf("data: %s: attack-types and armor-types must be non-empty", file)
	}
	seen := map[string]bool{}
	for _, s := range append(append([]string{}, raw.AttackTypes...), raw.ArmorTypes...) {
		if seen[s] {
			return fmt.Errorf("data: %s: duplicate type name %q", file, s)
		}
		seen[s] = true
	}
	if len(raw.Coefficients) != len(raw.AttackTypes) {
		return fmt.Errorf("data: %s: coefficients has %d rows, want one per attack type (%d)",
			file, len(raw.Coefficients), len(raw.AttackTypes))
	}
	t.AttackTypes = raw.AttackTypes
	t.ArmorTypes = raw.ArmorTypes
	t.Coeff = make([][]int32, len(raw.AttackTypes))
	for i, at := range raw.AttackTypes { // list order: deterministic
		row, ok := raw.Coefficients[at]
		if !ok {
			return fmt.Errorf("data: %s: coefficients missing row for attack type %q", file, at)
		}
		if len(row) != len(raw.ArmorTypes) {
			return fmt.Errorf("data: %s: row %q has %d values, want %d (one per armor type)",
				file, at, len(row), len(raw.ArmorTypes))
		}
		t.Coeff[i] = make([]int32, len(row))
		for j, v := range row {
			if v < 0 || v > 10000 {
				return fmt.Errorf("data: %s: coefficient %s vs %s = %d out of range [0, 10000] per-mille",
					file, at, raw.ArmorTypes[j], v)
			}
			t.Coeff[i][j] = int32(v)
		}
	}
	return nil
}

func (t *Tables) convertUnit(file string, r *rawUnit) (Unit, error) {
	fail := func(field string, err error) (Unit, error) {
		return Unit{}, fmt.Errorf("data: %s: unit %q: %s: %w", file, r.ID, field, err)
	}
	if r.ID == "" {
		return Unit{}, fmt.Errorf("data: %s: unit with empty id", file)
	}
	if r.Life <= 0 || r.Life > 1_000_000 {
		return fail("life", fmt.Errorf("%d out of range [1, 1000000]", r.Life))
	}
	if r.Armor < -20 || r.Armor > 100 {
		return fail("armor", fmt.Errorf("%d out of range [-20, 100]", r.Armor))
	}
	at := indexOf(t.ArmorTypes, r.ArmorType)
	if at < 0 {
		return fail("armor-type", fmt.Errorf("%q is not in the damage table %v", r.ArmorType, t.ArmorTypes))
	}
	regen, err := perSecondToPerTick(r.Regen)
	if err != nil {
		return fail("regen", err)
	}
	speed, err := perSecondToPerTick(r.MoveSpeed)
	if err != nil {
		return fail("move-speed", err)
	}
	turn, err := turnRateToBAM(r.TurnRate)
	if err != nil {
		return fail("turn-rate", err)
	}
	if r.CollisionSize < 0 || r.CollisionSize > 128 {
		return fail("collision-size", fmt.Errorf("%d out of range [0, 128]", r.CollisionSize))
	}
	var pathing uint8
	switch r.Pathing {
	case "ground":
		pathing = PathingGround
	case "air":
		pathing = PathingAir
	default:
		return fail("pathing", fmt.Errorf("%q is not ground|air", r.Pathing))
	}
	acq, err := worldUnits(r.AcquisitionRange)
	if err != nil {
		return fail("acquisition-range", err)
	}
	sightDay, err := worldUnits(r.SightDay)
	if err != nil {
		return fail("sight-day", err)
	}
	sightNight, err := worldUnits(r.SightNight)
	if err != nil {
		return fail("sight-night", err)
	}
	flyHeight, err := worldUnits(r.FlyHeight)
	if err != nil {
		return fail("fly-height", err)
	}
	if r.PointValue < 0 || r.PointValue > 1_000_000 {
		return fail("point-value", fmt.Errorf("%d out of range [0, 1000000]", r.PointValue))
	}
	if r.Level < 0 || r.Level > 1000 {
		return fail("level", fmt.Errorf("%d out of range [0, 1000]", r.Level))
	}
	var race uint8
	switch r.Race {
	case "":
		race = RaceNone
	case "human":
		race = RaceHuman
	case "orc":
		race = RaceOrc
	case "undead":
		race = RaceUndead
	case "nightelf":
		race = RaceNightElf
	case "demon":
		race = RaceDemon
	case "other":
		race = RaceOther
	default:
		return fail("race", fmt.Errorf("%q is not human|orc|undead|nightelf|demon|other", r.Race))
	}
	u := Unit{
		ID:               r.ID,
		Life:             int32(r.Life),
		RegenPerTick:     regen,
		Armor:            int32(r.Armor),
		ArmorType:        uint8(at),
		MoveSpeedPerTick: speed,
		TurnRatePerTick:  turn,
		CollisionSize:    int32(r.CollisionSize),
		CollisionClass:   cellRadius(int32(r.CollisionSize)),
		Pathing:          pathing,
		AcquisitionRange: acq,
		SightDay:         sightDay,
		SightNight:       sightNight,
		FlyHeight:        flyHeight,
		Model:            r.Model,
		Name:             r.Name,
		PointValue:       int32(r.PointValue),
		Level:            int32(r.Level),
		Race:             race,
	}
	if u.FoodCost, u.FoodProvided, u.DepotMask, u.Harvest, err = t.convertEconomy(file, r.ID, r); err != nil {
		return Unit{}, err
	}
	if u.Costs, u.TrainTicks, err = t.convertProduction(file, r.ID, r); err != nil {
		return Unit{}, err
	}
	u.RevivesHeroes = r.RevivesHeroes
	if u.Footprint, u.BuildTicks, u.RefundPermille, err = t.convertConstruction(file, r.ID, r); err != nil {
		return Unit{}, err
	}
	for ai := range r.Attacks {
		a, err := t.convertAttack(file, r.ID, &r.Attacks[ai])
		if err != nil {
			return Unit{}, err
		}
		u.Attacks = append(u.Attacks, a)
	}
	for _, ref := range r.Abilities {
		idx := sort.Search(len(t.Abilities), func(i int) bool { return t.Abilities[i].ID >= ref })
		if idx == len(t.Abilities) || t.Abilities[idx].ID != ref {
			return fail("abilities", fmt.Errorf("reference to undefined ability %q", ref))
		}
		u.Abilities = append(u.Abilities, uint16(idx))
	}
	return u, nil
}

func (t *Tables) convertAttack(file, unitID string, r *rawAttack) (Attack, error) {
	fail := func(field string, err error) (Attack, error) {
		return Attack{}, fmt.Errorf("data: %s: unit %q: attack %s: %w", file, unitID, field, err)
	}
	ti := indexOf(t.AttackTypes, r.Type)
	if ti < 0 {
		return fail("type", fmt.Errorf("%q is not in the damage table %v", r.Type, t.AttackTypes))
	}
	rng, err := worldUnits(r.Range)
	if err != nil {
		return fail("range", err)
	}
	if r.DamageBase < 0 || r.DamageBase > 100000 {
		return fail("damage-base", fmt.Errorf("%d out of range [0, 100000]", r.DamageBase))
	}
	if r.Dice < 0 || r.Dice > 100 {
		return fail("dice", fmt.Errorf("%d out of range [0, 100]", r.Dice))
	}
	if r.Dice > 0 && (r.Sides < 1 || r.Sides > 1000) {
		return fail("sides", fmt.Errorf("%d out of range [1, 1000] with dice > 0", r.Sides))
	}
	cd, err := SecondsToTicks(r.Cooldown)
	if err != nil {
		return fail("cooldown", err)
	}
	if cd == 0 {
		return fail("cooldown", fmt.Errorf("must be positive"))
	}
	dp, err := SecondsToTicks(r.DamagePoint)
	if err != nil {
		return fail("damage-point", err)
	}
	bs, err := SecondsToTicks(r.Backswing)
	if err != nil {
		return fail("backswing", err)
	}
	a := Attack{
		AttackType:       uint8(ti),
		Range:            rng,
		DamageBase:       int32(r.DamageBase),
		Dice:             int32(r.Dice),
		Sides:            int32(r.Sides),
		CooldownTicks:    cd,
		DamagePointTicks: dp,
		BackswingTicks:   bs,
	}
	switch r.Delivery {
	case "instant":
		a.Delivery = DeliveryInstant
		if r.ProjectileSpeed != 0 {
			return fail("projectile-speed", fmt.Errorf("set on an instant attack"))
		}
	case "projectile":
		a.Delivery = DeliveryProjectile
		ps, err := perSecondToPerTick(r.ProjectileSpeed)
		if err != nil {
			return fail("projectile-speed", err)
		}
		if ps <= 0 {
			return fail("projectile-speed", fmt.Errorf("must be positive for projectile delivery"))
		}
		a.ProjectileSpeedPerTick = ps
	default:
		return fail("delivery", fmt.Errorf("%q is not instant|projectile", r.Delivery))
	}
	if len(r.TargetsAllowed) == 0 {
		return fail("targets-allowed", fmt.Errorf("must list at least one target class"))
	}
	for _, tc := range r.TargetsAllowed {
		switch tc {
		case "ground":
			a.TargetsAllowed |= TargetGround
		case "air":
			a.TargetsAllowed |= TargetAir
		case "structure":
			a.TargetsAllowed |= TargetStructure
		default:
			return fail("targets-allowed", fmt.Errorf("%q is not ground|air|structure", tc))
		}
	}
	return a, nil
}

// ---- fingerprint ----

func writeString(h *statehash.Hasher, s string) {
	h.WriteU32(uint32(len(s)))
	h.WriteBytes([]byte(s))
}

// fingerprint hashes the converted schema in fixed field order: key
// order in the source never matters, every value does.
func (t *Tables) fingerprint() uint64 {
	h := statehash.New()
	h.WriteU32(FormatVersion)
	h.WriteU32(uint32(len(t.AttackTypes)))
	for _, s := range t.AttackTypes {
		writeString(h, s)
	}
	h.WriteU32(uint32(len(t.ArmorTypes)))
	for _, s := range t.ArmorTypes {
		writeString(h, s)
	}
	for _, row := range t.Coeff {
		for _, v := range row {
			h.WriteU32(uint32(v))
		}
	}
	h.WriteU32(uint32(len(t.Abilities)))
	for i := range t.Abilities {
		a := &t.Abilities[i]
		writeString(h, a.ID)
		writeString(h, a.Name)
		h.WriteU16(a.Effects.Off)
		h.WriteU16(a.Effects.Len)
		h.WriteU32(uint32(a.ManaCost))
		h.WriteU16(a.CooldownTicks)
		h.WriteU16(a.CastPointTicks)
		h.WriteU16(a.BackswingTicks)
		h.WriteU16(a.ChannelTicks)
		h.WriteI64(int64(a.CastRange))
	}
	hashEffects(h, t.Effects)
	t.hashBuffs(h)
	h.WriteU32(uint32(len(t.Units)))
	for i := range t.Units {
		u := &t.Units[i]
		writeString(h, u.ID)
		h.WriteU32(uint32(u.Life))
		h.WriteI64(int64(u.RegenPerTick))
		h.WriteU32(uint32(u.Armor))
		h.WriteU8(u.ArmorType)
		h.WriteI64(int64(u.MoveSpeedPerTick))
		h.WriteU16(uint16(u.TurnRatePerTick))
		h.WriteU32(uint32(u.CollisionSize))
		h.WriteU8(u.CollisionClass)
		h.WriteU8(u.Pathing)
		h.WriteI64(int64(u.AcquisitionRange))
		h.WriteI64(int64(u.SightDay))
		h.WriteI64(int64(u.SightNight))
		writeString(h, u.Model)
		h.WriteU32(uint32(len(u.Attacks)))
		for j := range u.Attacks {
			a := &u.Attacks[j]
			h.WriteU8(a.AttackType)
			h.WriteI64(int64(a.Range))
			h.WriteU32(uint32(a.DamageBase))
			h.WriteU32(uint32(a.Dice))
			h.WriteU32(uint32(a.Sides))
			h.WriteU16(a.CooldownTicks)
			h.WriteU16(a.DamagePointTicks)
			h.WriteU16(a.BackswingTicks)
			h.WriteU8(a.Delivery)
			h.WriteI64(int64(a.ProjectileSpeedPerTick))
			h.WriteU16(a.TargetsAllowed)
		}
		h.WriteU32(uint32(len(u.Abilities)))
		for _, ab := range u.Abilities {
			h.WriteU16(ab)
		}
		h.WriteU8(u.FoodCost)
		h.WriteU8(u.FoodProvided)
		h.WriteU16(u.DepotMask)
		h.WriteU16(u.Harvest.GatherTicks)
		h.WriteU32(uint32(u.Harvest.Capacity))
		h.WriteU16(u.Harvest.Mask)
		h.WriteU32(uint32(len(u.Costs)))
		for _, cv := range u.Costs {
			h.WriteI64(cv)
		}
		h.WriteU16(u.TrainTicks)
		h.WriteU32(uint32(len(u.Trains)))
		for _, tr := range u.Trains {
			h.WriteU16(tr)
		}
		h.WriteU32(uint32(len(u.Researches)))
		for _, rs := range u.Researches {
			h.WriteU16(rs)
		}
		h.WriteBool(u.RevivesHeroes)
		h.WriteU8(u.Footprint)
		h.WriteU16(u.BuildTicks)
		h.WriteU16(u.RefundPermille)
	}
	t.hashEconomy(h)
	t.hashTech(h)
	t.hashHero(h)
	t.hashItems(h)
	h.WriteBool(t.Smart != nil)
	if t.Smart != nil {
		t.Smart.hashInto(h)
	}
	return h.Sum64()
}

// String renders a unit row for FSV dumps — raw integer values only.
func (u *Unit) String() string {
	var b strings.Builder
	fmt.Fprintf(&b, "unit %q: life=%d regen/tick=%d armor=%d armorType=%d speed/tick=%d turn/tick=%d coll=%d class=%d pathing=%d acq=%d sightDay=%d sightNight=%d model=%q",
		u.ID, u.Life, int64(u.RegenPerTick), u.Armor, u.ArmorType,
		int64(u.MoveSpeedPerTick), uint16(u.TurnRatePerTick), u.CollisionSize,
		u.CollisionClass, u.Pathing, int64(u.AcquisitionRange), int64(u.SightDay),
		int64(u.SightNight), u.Model)
	for i := range u.Attacks {
		a := &u.Attacks[i]
		fmt.Fprintf(&b, "\n  attack[%d]: type=%d range=%d dmg=%d+%dd%d cd=%dt dp=%dt bs=%dt delivery=%d projSpeed/tick=%d targets=%03b",
			i, a.AttackType, int64(a.Range), a.DamageBase, a.Dice, a.Sides,
			a.CooldownTicks, a.DamagePointTicks, a.BackswingTicks, a.Delivery,
			int64(a.ProjectileSpeedPerTick), a.TargetsAllowed)
	}
	return b.String()
}

// convertAbilityCast converts one raw ability's cast block to sim
// units (seconds → ticks, world units → fixed) — fail closed on any
// out-of-range value.
func convertAbilityCast(file string, r *rawAbility) (Ability, error) {
	fail := func(field string, err error) (Ability, error) {
		return Ability{}, fmt.Errorf("data: %s: ability %q: %s: %w", file, r.ID, field, err)
	}
	if r.ManaCost < 0 || r.ManaCost > 1<<20 {
		return fail("mana-cost", fmt.Errorf("value %d out of range [0, 2^20]", r.ManaCost))
	}
	cd, err := SecondsToTicks(r.Cooldown)
	if err != nil {
		return fail("cooldown", err)
	}
	cp, err := SecondsToTicks(r.CastPoint)
	if err != nil {
		return fail("cast-point", err)
	}
	bs, err := SecondsToTicks(r.Backswing)
	if err != nil {
		return fail("backswing", err)
	}
	ch, err := SecondsToTicks(r.Channel)
	if err != nil {
		return fail("channel", err)
	}
	rng, err := worldUnits(r.CastRange)
	if err != nil {
		return fail("cast-range", err)
	}
	return Ability{
		ID: r.ID, Name: r.Name,
		ManaCost:       int32(r.ManaCost),
		CooldownTicks:  cd,
		CastPointTicks: cp,
		BackswingTicks: bs,
		ChannelTicks:   ch,
		CastRange:      rng,
	}, nil
}
