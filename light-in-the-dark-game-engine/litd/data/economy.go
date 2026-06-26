package data

// Economy tables (#300, R-AST-1): the resource-type registry, the
// resource-node type rows, and the per-unit economy block (harvest
// spec, food, depot acceptance). Lives in economy/resources.toml;
// absence is a visible empty registry, never a silent default. All
// decode is fail-closed; everything folds into the fingerprint.

import (
	"fmt"
	"io/fs"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// MaxResourceTypes bounds the registry: resource sets are uint16
// bitmasks in unit rows.
const MaxResourceTypes = 16

// ResourceNodeType is one converted node row.
type ResourceNodeType struct {
	ID        string
	Resource  uint8 // index into Tables.ResourceTypes
	Amount    int64 // initial remaining amount
	Exclusive bool  // one gatherer at a time (table flag; #300 edge 4)
}

// HarvestSpec is a unit's harvest capability. A zero Capacity means
// "not a harvester".
type HarvestSpec struct {
	GatherTicks uint16 // ticks per gather cycle
	Capacity    int32  // amount carried per trip
	Mask        uint16 // bit per harvestable resource index
}

type rawEconomyFile struct {
	ResourceTypes []string  `toml:"resource-types" json:"resource-types"`
	Node          []rawNode `toml:"node" json:"node"`
}

type rawNode struct {
	ID        string `toml:"id" json:"id"`
	Resource  string `toml:"resource" json:"resource"`
	Amount    int64  `toml:"amount" json:"amount"`
	Exclusive bool   `toml:"exclusive" json:"exclusive"`
}

type rawHarvest struct {
	GatherSeconds float64  `toml:"gather-seconds" json:"gather-seconds"`
	Capacity      int64    `toml:"capacity" json:"capacity"`
	Resources     []string `toml:"resources" json:"resources"`
}

// loadEconomy reads economy/resources.toml (optional — a missing
// directory is an empty registry).
func (t *Tables) loadEconomy(fsys fs.FS) error {
	files, err := listTables(fsys, "economy")
	if err != nil || len(files) == 0 {
		return nil
	}
	file, blob, err := readOne(fsys, "economy", "resources")
	if err != nil {
		return err
	}
	var raw rawEconomyFile
	if err := decodeStrict(file, blob, &raw); err != nil {
		return err
	}
	if len(raw.ResourceTypes) == 0 {
		return fmt.Errorf("data: %s: resource-types must be non-empty", file)
	}
	if len(raw.ResourceTypes) > MaxResourceTypes {
		return fmt.Errorf("data: %s: %d resource types exceeds limit %d", file, len(raw.ResourceTypes), MaxResourceTypes)
	}
	seen := map[string]bool{}
	for _, s := range raw.ResourceTypes {
		if s == "" || seen[s] {
			return fmt.Errorf("data: %s: empty or duplicate resource type %q", file, s)
		}
		seen[s] = true
	}
	t.ResourceTypes = raw.ResourceTypes
	for i := range raw.Node {
		n := &raw.Node[i]
		if n.ID == "" {
			return fmt.Errorf("data: %s: node with empty id", file)
		}
		res := indexOf(t.ResourceTypes, n.Resource)
		if res < 0 {
			return fmt.Errorf("data: %s: node %q: resource %q is not in resource-types %v", file, n.ID, n.Resource, t.ResourceTypes)
		}
		if n.Amount <= 0 || n.Amount > 1_000_000_000 {
			return fmt.Errorf("data: %s: node %q: amount %d out of range [1, 1e9]", file, n.ID, n.Amount)
		}
		t.Nodes = append(t.Nodes, ResourceNodeType{
			ID: n.ID, Resource: uint8(res), Amount: n.Amount, Exclusive: n.Exclusive,
		})
	}
	for i := 1; i < len(t.Nodes); i++ {
		if t.Nodes[i].ID == t.Nodes[i-1].ID {
			return fmt.Errorf("data: %s: duplicate node id %q", file, t.Nodes[i].ID)
		}
	}
	return nil
}

// convertEconomy validates a unit's economy block against the loaded
// registry (called from convertUnit).
func (t *Tables) convertEconomy(file, unitID string, r *rawUnit) (food uint8, provided uint8, depot uint16, hv HarvestSpec, err error) {
	fail := func(field string, e error) (uint8, uint8, uint16, HarvestSpec, error) {
		return 0, 0, 0, HarvestSpec{}, fmt.Errorf("data: %s: unit %q: %s: %w", file, unitID, field, e)
	}
	if r.FoodCost < 0 || r.FoodCost > 100 {
		return fail("food-cost", fmt.Errorf("%d out of range [0, 100]", r.FoodCost))
	}
	if r.FoodProvided < 0 || r.FoodProvided > 100 {
		return fail("food-provided", fmt.Errorf("%d out of range [0, 100]", r.FoodProvided))
	}
	for _, name := range r.DepotFor {
		res := indexOf(t.ResourceTypes, name)
		if res < 0 {
			return fail("depot-for", fmt.Errorf("resource %q is not in resource-types %v", name, t.ResourceTypes))
		}
		depot |= 1 << uint(res)
	}
	if r.Harvest != nil {
		h := r.Harvest
		ticks, e := SecondsToTicks(h.GatherSeconds)
		if e != nil || ticks == 0 {
			return fail("harvest.gather-seconds", fmt.Errorf("%v (must be ≥ one tick)", h.GatherSeconds))
		}
		if h.Capacity <= 0 || h.Capacity > 100_000 {
			return fail("harvest.capacity", fmt.Errorf("%d out of range [1, 100000]", h.Capacity))
		}
		if len(h.Resources) == 0 {
			return fail("harvest.resources", fmt.Errorf("must name at least one resource"))
		}
		for _, name := range h.Resources {
			res := indexOf(t.ResourceTypes, name)
			if res < 0 {
				return fail("harvest.resources", fmt.Errorf("resource %q is not in resource-types %v", name, t.ResourceTypes))
			}
			hv.Mask |= 1 << uint(res)
		}
		hv.GatherTicks = ticks
		hv.Capacity = int32(h.Capacity)
	}
	return uint8(r.FoodCost), uint8(r.FoodProvided), depot, hv, nil
}

// convertProduction validates a unit's production block (#302):
// costs keyed by registry resource names, train time on the tick
// grid. Trains lists resolve post-sort in Load.
func (t *Tables) convertProduction(file, unitID string, r *rawUnit) (costs []int64, trainTicks uint16, err error) {
	fail := func(field string, e error) ([]int64, uint16, error) {
		return nil, 0, fmt.Errorf("data: %s: unit %q: %s: %w", file, unitID, field, e)
	}
	if len(r.Costs) > 0 {
		costs = make([]int64, len(t.ResourceTypes))
		for name, v := range r.Costs {
			res := indexOf(t.ResourceTypes, name)
			if res < 0 {
				return fail("costs", fmt.Errorf("resource %q is not in resource-types %v", name, t.ResourceTypes))
			}
			if v < 0 || v > 1_000_000 {
				return fail("costs", fmt.Errorf("%s = %d out of range [0, 1e6]", name, v))
			}
			costs[res] = v
		}
	}
	if r.TrainSeconds != 0 {
		ticks, e := SecondsToTicks(r.TrainSeconds)
		if e != nil || ticks == 0 {
			return fail("train-seconds", fmt.Errorf("%v", r.TrainSeconds))
		}
		trainTicks = ticks
	}
	return costs, trainTicks, nil
}

// convertConstruction validates the building/construction block (#301).
// A unit is constructable iff build-seconds > 0; such a unit must
// declare a positive footprint. Refund is a per-mille fraction.
func (t *Tables) convertConstruction(file, unitID string, r *rawUnit) (footprint uint8, buildTicks uint16, refund uint16, err error) {
	fail := func(field string, e error) (uint8, uint16, uint16, error) {
		return 0, 0, 0, fmt.Errorf("data: %s: unit %q: %s: %w", file, unitID, field, e)
	}
	if r.Footprint < 0 || r.Footprint > 64 {
		return fail("footprint", fmt.Errorf("%d out of range [0, 64]", r.Footprint))
	}
	footprint = uint8(r.Footprint)
	if r.RefundPermille < 0 || r.RefundPermille > 1000 {
		return fail("refund-permille", fmt.Errorf("%d out of range [0, 1000]", r.RefundPermille))
	}
	refund = uint16(r.RefundPermille)
	if r.BuildSeconds != 0 {
		ticks, e := SecondsToTicks(r.BuildSeconds)
		if e != nil || ticks == 0 {
			return fail("build-seconds", fmt.Errorf("%v", r.BuildSeconds))
		}
		buildTicks = ticks
		if footprint == 0 {
			return fail("footprint", fmt.Errorf("a constructable building (build-seconds > 0) needs a positive footprint"))
		}
	}
	return footprint, buildTicks, refund, nil
}

// hashEconomy folds the registry, nodes, and is called from the
// fingerprint (unit econ fields fold with their unit rows).
func (t *Tables) hashEconomy(h *statehash.Hasher) {
	h.WriteU32(uint32(len(t.ResourceTypes)))
	for _, s := range t.ResourceTypes {
		writeString(h, s)
	}
	h.WriteU32(uint32(len(t.Nodes)))
	for i := range t.Nodes {
		n := &t.Nodes[i]
		writeString(h, n.ID)
		h.WriteU8(n.Resource)
		h.WriteI64(n.Amount)
		h.WriteBool(n.Exclusive)
	}
}
