package data

// Effect-model registry (#530): the world-data path that makes
// Game_AddSpecialEffect usable from an authored world. Each row binds a script
// model key (the string passed to AddSpecialEffect) to a render asset path; the
// loader assigns each a deterministic numeric sim ModelID at install (worldhost),
// so a world's main.lua can spawn a live special effect without a Go rebuild.
// Lives in effects-models/models.toml; absence is a visible empty registry,
// never a silent default. Decode is fail-closed; a present table folds into the
// fingerprint.

import (
	"fmt"
	"io/fs"
	"sort"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// MaxEffectModels bounds the registry. IDs are assigned 1..N at install (0 =
// invalid/unregistered) and carried as uint16 sim ModelIDs, so the registry
// stays well within the uint16 space.
const MaxEffectModels = 1024

// EffectModel is one registered special-effect model. Key is the handle passed
// to Game_AddSpecialEffect; Asset is the render model path the render layer
// resolves for that key (the sim carries only the assigned numeric id).
type EffectModel struct {
	Key   string
	Asset string
}

type rawEffectModelsFile struct {
	Model []rawEffectModel `toml:"model" json:"model"`
}

type rawEffectModel struct {
	Key   string `toml:"key" json:"key"`
	Asset string `toml:"asset" json:"asset"`
}

// loadEffectModels reads effects-models/models.{toml,json} (optional — a missing
// directory is an empty registry). Rows are sorted by Key so the load-time id
// assignment (1..N in worldhost) is independent of source row order — a
// determinism requirement (R-SIM-2).
func (t *Tables) loadEffectModels(fsys fs.FS) error {
	files, err := listTables(fsys, "effects-models")
	if err != nil || len(files) == 0 {
		return nil
	}
	file, blob, err := readOne(fsys, "effects-models", "models")
	if err != nil {
		return err
	}
	var raw rawEffectModelsFile
	if err := decodeStrict(file, blob, &raw); err != nil {
		return err
	}
	if len(raw.Model) == 0 {
		return fmt.Errorf("data: %s: effects-models must declare at least one [[model]]", file)
	}
	if len(raw.Model) > MaxEffectModels {
		return fmt.Errorf("data: %s: %d effect models exceeds limit %d", file, len(raw.Model), MaxEffectModels)
	}
	models := make([]EffectModel, 0, len(raw.Model))
	for i := range raw.Model {
		m := &raw.Model[i]
		if m.Key == "" {
			return fmt.Errorf("data: %s: model with empty key", file)
		}
		if m.Asset == "" {
			return fmt.Errorf("data: %s: model %q: asset must be non-empty", file, m.Key)
		}
		models = append(models, EffectModel{Key: m.Key, Asset: m.Asset})
	}
	sort.Slice(models, func(i, j int) bool { return models[i].Key < models[j].Key })
	for i := 1; i < len(models); i++ {
		if models[i].Key == models[i-1].Key {
			return fmt.Errorf("data: %s: duplicate model key %q", file, models[i].Key)
		}
	}
	t.EffectModels = models
	return nil
}

// hashEffectModels folds the registry into the fingerprint. Like hashPlacement
// it writes NOTHING when the registry is empty, so a world without an
// effects-models table keeps its exact prior fingerprint (existing goldens
// unchanged); only a world that ships models hashes differently.
func (t *Tables) hashEffectModels(h *statehash.Hasher) {
	if len(t.EffectModels) == 0 {
		return
	}
	h.WriteU32(uint32(len(t.EffectModels)))
	for i := range t.EffectModels {
		m := &t.EffectModels[i]
		writeString(h, m.Key)
		writeString(h, m.Asset)
	}
}
