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
