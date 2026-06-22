package shell

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"regexp"
	"sort"
	"strconv"
	"strings"
)

var factionIDPattern = regexp.MustCompile(`^[a-z0-9][a-z0-9_-]{1,63}$`)

type FactionCreatorDraft struct {
	ID        string   `json:"id"`
	Name      string   `json:"name"`
	Culture   string   `json:"culture"`
	Traits    []string `json:"traits"`
	Grimoires []string `json:"grimoires"`
}

type FactionCreatorState struct {
	Draft       FactionCreatorDraft
	LastOutputs []FactionOutput
	LastError   string
}

type FactionCreatorSnapshot struct {
	Draft       FactionCreatorDraft `json:"draft"`
	Catalog     FactionCatalog      `json:"catalog"`
	Valid       bool                `json:"valid"`
	Errors      []string            `json:"errors,omitempty"`
	Preview     FactionPreview      `json:"preview"`
	LastOutputs []FactionOutput     `json:"lastOutputs,omitempty"`
	LastError   string              `json:"lastError,omitempty"`
}

type FactionCatalog struct {
	Cultures  []FactionCulture  `json:"cultures"`
	Traits    []FactionTrait    `json:"traits"`
	Grimoires []FactionGrimoire `json:"grimoires"`
}

type FactionCulture struct {
	ID          string         `json:"id"`
	Name        string         `json:"name"`
	TownHall    string         `json:"townHall"`
	Worker      string         `json:"worker"`
	WorkerCount int            `json:"workerCount"`
	Gold        int            `json:"gold"`
	Lumber      int            `json:"lumber"`
	FoodCap     int            `json:"foodCap"`
	Extra       []FactionSquad `json:"extra,omitempty"`
}

type FactionTrait struct {
	ID        string   `json:"id"`
	Name      string   `json:"name"`
	Conflicts []string `json:"conflicts,omitempty"`
}

type FactionGrimoire struct {
	ID   string `json:"id"`
	Name string `json:"name"`
}

type FactionSquad struct {
	Code  string `json:"code"`
	Count int    `json:"count"`
}

type FactionPreview struct {
	ID          string         `json:"id"`
	Name        string         `json:"name"`
	Culture     string         `json:"culture"`
	TownHall    string         `json:"townHall"`
	Worker      string         `json:"worker"`
	WorkerCount int            `json:"workerCount"`
	Gold        int            `json:"gold"`
	Lumber      int            `json:"lumber"`
	FoodCap     int            `json:"foodCap"`
	Extra       []FactionSquad `json:"extra,omitempty"`
	OutputPaths []string       `json:"outputPaths,omitempty"`
}

type FactionOutput struct {
	Path   string `json:"path"`
	Bytes  int    `json:"bytes"`
	SHA256 string `json:"sha256,omitempty"`
}

type factionBuild struct {
	draft   FactionCreatorDraft
	culture FactionCulture
	traits  []FactionTrait
	grims   []FactionGrimoire
	preview FactionPreview
	files   []factionFile
}

type factionFile struct {
	path string
	body []byte
}

func FactionCreatorCatalog() FactionCatalog {
	return FactionCatalog{
		Cultures:  cloneCultures(factionCultures),
		Traits:    cloneTraits(factionTraits),
		Grimoires: cloneGrimoires(factionGrimoires),
	}
}

func (a *App) SaveFactionDraft(draft FactionCreatorDraft) (FactionCreatorSnapshot, error) {
	if a.world == nil {
		return FactionCreatorSnapshot{}, fmt.Errorf("editor faction: no project loaded")
	}
	if a.archiveReadOnly {
		err := fmt.Errorf("editor faction: archive opened read-only; source-form payload required for faction creation")
		a.errText = err.Error()
		a.status = a.errText
		return a.FactionCreatorSnapshot(), err
	}
	build, err := buildFaction(draft)
	if err != nil {
		a.faction.Draft = normalizeFactionDraft(draft)
		a.faction.LastOutputs = nil
		a.faction.LastError = err.Error()
		a.errText = err.Error()
		a.status = a.errText
		a.mode = ModeFaction
		return a.FactionCreatorSnapshot(), err
	}
	for _, f := range build.files {
		if err := a.world.SetPassthroughFile(f.path, f.body); err != nil {
			a.faction.LastError = err.Error()
			a.errText = err.Error()
			a.status = a.errText
			return a.FactionCreatorSnapshot(), err
		}
	}
	a.faction.Draft = build.draft
	a.faction.LastOutputs = outputsFor(build.files)
	a.faction.LastError = ""
	a.errText = ""
	a.mode = ModeFaction
	a.status = fmt.Sprintf("Faction saved: %s", build.draft.Name)
	return a.FactionCreatorSnapshot(), nil
}

func (a *App) FactionCreatorSnapshot() FactionCreatorSnapshot {
	draft := a.faction.Draft
	empty := factionDraftEmpty(draft)
	if empty {
		draft = FactionCreatorDraft{Culture: factionCultures[0].ID}
	}
	snap := FactionCreatorSnapshot{
		Draft:       normalizeFactionDraft(draft),
		Catalog:     FactionCreatorCatalog(),
		LastOutputs: append([]FactionOutput(nil), a.faction.LastOutputs...),
		LastError:   a.faction.LastError,
	}
	if empty {
		return snap
	}
	build, err := buildFaction(draft)
	if err != nil {
		snap.Errors = strings.Split(err.Error(), "; ")
		return snap
	}
	snap.Valid = true
	snap.Preview = build.preview
	return snap
}

func factionDraftEmpty(d FactionCreatorDraft) bool {
	return d.ID == "" && d.Name == "" && d.Culture == "" && len(d.Traits) == 0 && len(d.Grimoires) == 0
}

func buildFaction(draft FactionCreatorDraft) (factionBuild, error) {
	draft = normalizeFactionDraft(draft)
	var errs []string
	if !factionIDPattern.MatchString(draft.ID) {
		errs = append(errs, fmt.Sprintf("id %q must match %s", draft.ID, factionIDPattern.String()))
	}
	if draft.Name == "" {
		errs = append(errs, "name is required")
	}
	culture, ok := findCulture(draft.Culture)
	if !ok {
		errs = append(errs, fmt.Sprintf("unknown culture %q", draft.Culture))
	}
	traits, traitErrs := resolveTraits(draft.Traits)
	errs = append(errs, traitErrs...)
	grims, grimErrs := resolveGrimoires(draft.Grimoires)
	errs = append(errs, grimErrs...)
	if len(draft.Grimoires) == 0 {
		errs = append(errs, "at least one grimoire track is required")
	}
	if len(errs) > 0 {
		return factionBuild{}, fmt.Errorf("editor faction: %s", strings.Join(errs, "; "))
	}

	meleePath := "data/melee/" + draft.ID + ".toml"
	metaPath := "data/factions/" + draft.ID + ".toml"
	luaPath := "scripts/factions/" + draft.ID + ".lua"
	preview := FactionPreview{
		ID:          draft.ID,
		Name:        draft.Name,
		Culture:     culture.ID,
		TownHall:    culture.TownHall,
		Worker:      culture.Worker,
		WorkerCount: culture.WorkerCount,
		Gold:        culture.Gold,
		Lumber:      culture.Lumber,
		FoodCap:     culture.FoodCap,
		Extra:       cloneSquads(culture.Extra),
		OutputPaths: []string{meleePath, metaPath, luaPath},
	}
	files := []factionFile{
		{path: meleePath, body: renderFactionMeleeTable(draft, culture)},
		{path: metaPath, body: renderFactionMetadata(draft, culture, traits, grims, meleePath, luaPath)},
		{path: luaPath, body: renderFactionLua(draft, culture, traits, grims, meleePath)},
	}
	return factionBuild{draft: draft, culture: culture, traits: traits, grims: grims, preview: preview, files: files}, nil
}

func normalizeFactionDraft(d FactionCreatorDraft) FactionCreatorDraft {
	d.ID = strings.ToLower(strings.TrimSpace(d.ID))
	d.Name = strings.TrimSpace(d.Name)
	d.Culture = strings.ToLower(strings.TrimSpace(d.Culture))
	d.Traits = normalizeIDList(d.Traits)
	d.Grimoires = normalizeIDList(d.Grimoires)
	return d
}

func normalizeIDList(in []string) []string {
	out := make([]string, 0, len(in))
	for _, v := range in {
		v = strings.ToLower(strings.TrimSpace(v))
		if v != "" {
			out = append(out, v)
		}
	}
	return out
}

func resolveTraits(ids []string) ([]FactionTrait, []string) {
	seen := map[string]bool{}
	out := make([]FactionTrait, 0, len(ids))
	var errs []string
	for _, id := range ids {
		if seen[id] {
			errs = append(errs, fmt.Sprintf("trait %q is duplicated", id))
			continue
		}
		seen[id] = true
		tr, ok := findTrait(id)
		if !ok {
			errs = append(errs, fmt.Sprintf("unknown trait %q", id))
			continue
		}
		for _, conflict := range tr.Conflicts {
			if seen[conflict] {
				errs = append(errs, fmt.Sprintf("trait %q conflicts with %q", id, conflict))
			}
		}
		out = append(out, tr)
	}
	return out, errs
}

func resolveGrimoires(ids []string) ([]FactionGrimoire, []string) {
	seen := map[string]bool{}
	out := make([]FactionGrimoire, 0, len(ids))
	var errs []string
	for _, id := range ids {
		if seen[id] {
			errs = append(errs, fmt.Sprintf("grimoire %q is duplicated", id))
			continue
		}
		seen[id] = true
		gr, ok := findGrimoire(id)
		if !ok {
			errs = append(errs, fmt.Sprintf("unknown grimoire %q", id))
			continue
		}
		out = append(out, gr)
	}
	return out, errs
}

func renderFactionMeleeTable(d FactionCreatorDraft, c FactionCulture) []byte {
	var b strings.Builder
	b.WriteString("# Generated by Light in the Dark editor faction creator.\n")
	fmt.Fprintf(&b, "name = %s\n\n", strconv.Quote(d.Name))
	fmt.Fprintf(&b, "gold = %d\n", c.Gold)
	fmt.Fprintf(&b, "lumber = %d\n", c.Lumber)
	fmt.Fprintf(&b, "food_cap = %d\n\n", c.FoodCap)
	fmt.Fprintf(&b, "town_hall = %s\n\n", strconv.Quote(c.TownHall))
	if len(c.Extra) == 0 {
		b.WriteString("extra = []\n\n")
	}
	b.WriteString("[workers]\n")
	fmt.Fprintf(&b, "code = %s\n", strconv.Quote(c.Worker))
	fmt.Fprintf(&b, "count = %d\n", c.WorkerCount)
	for _, ex := range c.Extra {
		b.WriteString("\n[[extra]]\n")
		fmt.Fprintf(&b, "code = %s\n", strconv.Quote(ex.Code))
		fmt.Fprintf(&b, "count = %d\n", ex.Count)
	}
	return []byte(b.String())
}

func renderFactionMetadata(d FactionCreatorDraft, c FactionCulture, traits []FactionTrait, grims []FactionGrimoire, meleePath, luaPath string) []byte {
	var b strings.Builder
	b.WriteString("# Generated by Light in the Dark editor faction creator.\n")
	b.WriteString("schema = 1\n")
	fmt.Fprintf(&b, "id = %s\n", strconv.Quote(d.ID))
	fmt.Fprintf(&b, "name = %s\n", strconv.Quote(d.Name))
	fmt.Fprintf(&b, "culture = %s\n", strconv.Quote(c.ID))
	fmt.Fprintf(&b, "traits = %s\n", quoteIDList(traitIDs(traits)))
	fmt.Fprintf(&b, "grimoires = %s\n", quoteIDList(grimoireIDs(grims)))
	fmt.Fprintf(&b, "melee_table = %s\n", strconv.Quote(meleePath))
	fmt.Fprintf(&b, "script = %s\n", strconv.Quote(luaPath))
	fmt.Fprintf(&b, "worker = %s\n", strconv.Quote(c.Worker))
	fmt.Fprintf(&b, "main_building = %s\n", strconv.Quote(c.TownHall))
	return []byte(b.String())
}

func renderFactionLua(d FactionCreatorDraft, c FactionCulture, traits []FactionTrait, grims []FactionGrimoire, meleePath string) []byte {
	var b strings.Builder
	b.WriteString("-- Generated by Light in the Dark editor faction creator.\n")
	b.WriteString("FactionDefinitions = FactionDefinitions or {}\n")
	fmt.Fprintf(&b, "FactionDefinitions[%s] = {\n", strconv.Quote(d.ID))
	fmt.Fprintf(&b, "  name = %s,\n", strconv.Quote(d.Name))
	fmt.Fprintf(&b, "  culture = %s,\n", strconv.Quote(c.ID))
	fmt.Fprintf(&b, "  traits = %s,\n", luaIDList(traitIDs(traits)))
	fmt.Fprintf(&b, "  grimoires = %s,\n", luaIDList(grimoireIDs(grims)))
	fmt.Fprintf(&b, "  meleeTable = %s,\n", strconv.Quote(meleePath))
	fmt.Fprintf(&b, "  townHall = %s,\n", strconv.Quote(c.TownHall))
	fmt.Fprintf(&b, "  worker = %s,\n", strconv.Quote(c.Worker))
	b.WriteString("}\n")
	fmt.Fprintf(&b, "return FactionDefinitions[%s]\n", strconv.Quote(d.ID))
	return []byte(b.String())
}

func quoteIDList(ids []string) string {
	var b strings.Builder
	b.WriteByte('[')
	for i, id := range ids {
		if i > 0 {
			b.WriteString(", ")
		}
		b.WriteString(strconv.Quote(id))
	}
	b.WriteByte(']')
	return b.String()
}

func luaIDList(ids []string) string {
	var b strings.Builder
	b.WriteByte('{')
	for i, id := range ids {
		if i > 0 {
			b.WriteString(", ")
		}
		b.WriteString(strconv.Quote(id))
	}
	b.WriteByte('}')
	return b.String()
}

func traitIDs(traits []FactionTrait) []string {
	out := make([]string, len(traits))
	for i, tr := range traits {
		out[i] = tr.ID
	}
	return out
}

func grimoireIDs(grims []FactionGrimoire) []string {
	out := make([]string, len(grims))
	for i, gr := range grims {
		out[i] = gr.ID
	}
	return out
}

func findCulture(id string) (FactionCulture, bool) {
	for _, c := range factionCultures {
		if c.ID == id {
			return c, true
		}
	}
	return FactionCulture{}, false
}

func findTrait(id string) (FactionTrait, bool) {
	for _, tr := range factionTraits {
		if tr.ID == id {
			return tr, true
		}
	}
	return FactionTrait{}, false
}

func findGrimoire(id string) (FactionGrimoire, bool) {
	for _, gr := range factionGrimoires {
		if gr.ID == id {
			return gr, true
		}
	}
	return FactionGrimoire{}, false
}

func cloneCultures(in []FactionCulture) []FactionCulture {
	out := append([]FactionCulture(nil), in...)
	for i := range out {
		out[i].Extra = cloneSquads(out[i].Extra)
	}
	return out
}

func cloneTraits(in []FactionTrait) []FactionTrait {
	out := append([]FactionTrait(nil), in...)
	for i := range out {
		out[i].Conflicts = append([]string(nil), out[i].Conflicts...)
	}
	return out
}

func cloneGrimoires(in []FactionGrimoire) []FactionGrimoire {
	return append([]FactionGrimoire(nil), in...)
}

func cloneSquads(in []FactionSquad) []FactionSquad {
	return append([]FactionSquad(nil), in...)
}

func outputsFor(files []factionFile) []FactionOutput {
	out := make([]FactionOutput, len(files))
	for i, f := range files {
		sum := sha256.Sum256(f.body)
		out[i] = FactionOutput{Path: f.path, Bytes: len(f.body), SHA256: hex.EncodeToString(sum[:])}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Path < out[j].Path })
	return out
}

var factionCultures = []FactionCulture{
	{ID: "vigil", Name: "The Vigil", TownHall: "htow", Worker: "hpea", WorkerCount: 5, Gold: 500, Lumber: 150, FoodCap: 12},
	{ID: "unbound", Name: "The Unbound", TownHall: "ugol", Worker: "uaco", WorkerCount: 5, Gold: 500, Lumber: 150, FoodCap: 11, Extra: []FactionSquad{{Code: "ushd", Count: 1}}},
}

var factionTraits = []FactionTrait{
	{ID: "beacon-stewards", Name: "Beacon Stewards", Conflicts: []string{"gloam-touched"}},
	{ID: "ember-raiders", Name: "Ember Raiders", Conflicts: []string{"rootbound"}},
	{ID: "gloam-touched", Name: "Gloam Touched", Conflicts: []string{"beacon-stewards"}},
	{ID: "rootbound", Name: "Rootbound", Conflicts: []string{"ember-raiders"}},
}

var factionGrimoires = []FactionGrimoire{
	{ID: "long-vigil", Name: "Grimoire of the Long Vigil"},
	{ID: "ember-road", Name: "Grimoire of the Ember Road"},
	{ID: "gloam-threshold", Name: "Grimoire of the Gloam Threshold"},
	{ID: "root-memory", Name: "Grimoire of Root Memory"},
}
