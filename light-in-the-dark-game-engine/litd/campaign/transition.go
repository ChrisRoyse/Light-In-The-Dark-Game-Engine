package campaign

import (
	"fmt"
	"strings"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

type HookOutcome string

const (
	OutcomeStart    HookOutcome = "start"
	OutcomeComplete HookOutcome = "complete"
	OutcomeFail     HookOutcome = "fail"
)

type HookResult struct {
	NextMissionID string
	Heroes        []HeroCarryOver
	CacheKeys     []string
	Log           []string
}

type Transition struct {
	CampaignID    string         `json:"campaignId"`
	FromMissionID string         `json:"fromMissionId"`
	Outcome       HookOutcome    `json:"outcome"`
	NextMissionID string         `json:"nextMissionId,omitempty"`
	CarryOver     CarryOver      `json:"carryOver"`
	Cache         []CarriedCache `json:"cache,omitempty"`
	Skipped       []string       `json:"skipped,omitempty"`
	HookLog       []string       `json:"hookLog,omitempty"`
	StoreCategory string         `json:"storeCategory"`
	CompleteAfter bool           `json:"completeAfter,omitempty"`
	FailedAfter   bool           `json:"failedAfter,omitempty"`
}

type CarriedCache struct {
	Key  string `json:"key"`
	Type string `json:"type"`
}

type cacheValue struct {
	kind string
	i    int
	r    float64
	s    string
	b    bool
}

func CommitHookResult(store, hookStore *api.Storage, def Definition, missionID string, outcome HookOutcome, result HookResult) (Transition, error) {
	if err := Validate(def); err != nil {
		return Transition{}, err
	}
	if store == nil {
		return Transition{}, fmt.Errorf("campaign: nil storage")
	}
	if hookStore == nil {
		hookStore = store
	}
	missionID = strings.TrimSpace(missionID)
	if !missionExists(def, missionID) {
		return Transition{}, fmt.Errorf("campaign: mission %q is not in campaign %s", missionID, def.ID)
	}
	if err := validateOutcome(outcome); err != nil {
		return Transition{}, err
	}

	next := strings.TrimSpace(result.NextMissionID)
	if next == "" && outcome != OutcomeStart {
		return Transition{}, fmt.Errorf("campaign: %s hook for mission %s did not select a next mission", outcome, missionID)
	}
	if next != "" && !missionExists(def, next) {
		return Transition{}, fmt.Errorf("campaign: %s hook for mission %s selected unknown mission %q", outcome, missionID, next)
	}

	carry, skipped, err := normalizeHookCarry(def, next, outcome, result.Heroes)
	if err != nil {
		return Transition{}, err
	}
	cacheKeys, err := normalizeHookCacheKeys(def, result.CacheKeys)
	if err != nil {
		return Transition{}, err
	}
	cacheValues, cacheSkipped, err := readHookCacheValues(hookStore, def.ID, cacheKeys)
	if err != nil {
		return Transition{}, err
	}
	skipped = append(skipped, cacheSkipped...)
	log := normalizeHookLog(result.Log, skipped)

	cat := category(def.ID)
	if outcome == OutcomeComplete {
		store.SetBool(cat, completeKey(missionID), true)
	}
	if outcome == OutcomeFail {
		store.SetBool(cat, failedKey(missionID), true)
	}
	if next != "" {
		store.SetString(cat, nextMissionKey(missionID), next)
	}
	if carry.MissionID != "" {
		if err := writeCarryOver(store, def, carry); err != nil {
			return Transition{}, err
		}
	}
	cache := writeCacheValues(store, def.ID, next, cacheValues)
	if len(log) > 0 {
		body := strings.Join(log, "\n")
		store.SetString(cat, hookLogKey(missionID, outcome), body)
		store.SetString(cat, "hook:last", body)
	}
	complete, _ := store.GetBool(cat, completeKey(missionID))
	failed, _ := store.GetBool(cat, failedKey(missionID))
	return Transition{
		CampaignID:    def.ID,
		FromMissionID: missionID,
		Outcome:       outcome,
		NextMissionID: next,
		CarryOver:     carry,
		Cache:         cache,
		Skipped:       skipped,
		HookLog:       log,
		StoreCategory: cat,
		CompleteAfter: complete,
		FailedAfter:   failed,
	}, nil
}

func validateOutcome(outcome HookOutcome) error {
	switch outcome {
	case OutcomeStart, OutcomeComplete, OutcomeFail:
		return nil
	default:
		return fmt.Errorf("campaign: unknown hook outcome %q", outcome)
	}
}

func normalizeHookCarry(def Definition, next string, outcome HookOutcome, heroes []HeroCarryOver) (CarryOver, []string, error) {
	if next == "" {
		if len(heroes) > 0 {
			return CarryOver{}, nil, fmt.Errorf("campaign: hook returned carry-over heroes without a next mission")
		}
		return CarryOver{}, nil, nil
	}
	if len(heroes) == 0 {
		if outcome == OutcomeComplete && len(def.Carry.Heroes) > 0 {
			return CarryOver{}, nil, fmt.Errorf("campaign: carry manifest hero %q was not returned by the hook", def.Carry.Heroes[0])
		}
		return CarryOver{MissionID: next}, nil, nil
	}
	allowedHeroes := setOf(def.Carry.Heroes)
	allowedItems := setOf(def.Carry.Items)
	seenHeroes := map[string]bool{}
	seenItems := map[string]bool{}
	out := CarryOver{MissionID: next, Heroes: make([]HeroCarryOver, 0, len(heroes))}
	for i, h := range heroes {
		h.Name = strings.TrimSpace(h.Name)
		if !allowedHeroes[h.Name] {
			return CarryOver{}, nil, fmt.Errorf("campaign: hook hero %d %q is not listed in the carry manifest", i, h.Name)
		}
		if seenHeroes[h.Name] {
			return CarryOver{}, nil, fmt.Errorf("campaign: hook hero %q is duplicated", h.Name)
		}
		seenHeroes[h.Name] = true
		filtered := h
		filtered.Items = filtered.Items[:0]
		localItems := map[string]bool{}
		for j, item := range h.Items {
			item = strings.TrimSpace(item)
			if !allowedItems[item] {
				return CarryOver{}, nil, fmt.Errorf("campaign: hook hero %s item %d %q is not listed in the carry manifest", h.Name, j, item)
			}
			if localItems[item] {
				return CarryOver{}, nil, fmt.Errorf("campaign: hook hero %s item %q is duplicated", h.Name, item)
			}
			localItems[item] = true
			seenItems[item] = true
			filtered.Items = append(filtered.Items, item)
		}
		out.Heroes = append(out.Heroes, filtered)
	}
	for _, hero := range def.Carry.Heroes {
		if outcome == OutcomeComplete && !seenHeroes[hero] {
			return CarryOver{}, nil, fmt.Errorf("campaign: carry manifest hero %q was not returned by the hook", hero)
		}
	}
	var skipped []string
	for _, item := range def.Carry.Items {
		if !seenItems[item] {
			skipped = append(skipped, fmt.Sprintf("item %q skipped: not held by returned carry-over heroes", item))
		}
	}
	if len(out.Heroes) == 0 {
		return CarryOver{MissionID: next}, skipped, nil
	}
	carry, err := normalizeCarryOver(def, out)
	if err != nil {
		return CarryOver{}, nil, err
	}
	return carry, skipped, nil
}

func normalizeHookCacheKeys(def Definition, keys []string) ([]string, error) {
	if len(keys) == 0 {
		return nil, nil
	}
	allowed := setOf(def.Carry.CacheKeys)
	seen := map[string]bool{}
	out := make([]string, 0, len(keys))
	for i, key := range keys {
		key = strings.TrimSpace(key)
		if err := validateManifestName("cache key", key); err != nil {
			return nil, fmt.Errorf("campaign: hook cache[%d]: %w", i, err)
		}
		if !allowed[key] {
			return nil, fmt.Errorf("campaign: hook cache key %q is not listed in the carry manifest", key)
		}
		if seen[key] {
			return nil, fmt.Errorf("campaign: hook cache key %q is duplicated", key)
		}
		seen[key] = true
		out = append(out, key)
	}
	return out, nil
}

func readHookCacheValues(store *api.Storage, campaignID string, keys []string) (map[string]cacheValue, []string, error) {
	values := map[string]cacheValue{}
	var skipped []string
	cat := category(campaignID)
	for _, key := range keys {
		src := CacheSourceKey(key)
		value, ok, err := readCacheValue(store, cat, src)
		if err != nil {
			return nil, nil, err
		}
		if !ok {
			skipped = append(skipped, fmt.Sprintf("cache key %q skipped: source %s/%s missing", key, cat, src))
			continue
		}
		values[key] = value
	}
	return values, skipped, nil
}

func readCacheValue(store *api.Storage, category, key string) (cacheValue, bool, error) {
	var out cacheValue
	found := 0
	if v, ok := store.GetString(category, key); ok {
		out = cacheValue{kind: "string", s: v}
		found++
	}
	if v, ok := store.GetInt(category, key); ok {
		out = cacheValue{kind: "int", i: v}
		found++
	}
	if v, ok := store.GetReal(category, key); ok {
		out = cacheValue{kind: "real", r: v}
		found++
	}
	if v, ok := store.GetBool(category, key); ok {
		out = cacheValue{kind: "bool", b: v}
		found++
	}
	if found > 1 {
		return cacheValue{}, false, fmt.Errorf("campaign: cache key %s/%s has multiple typed values", category, key)
	}
	return out, found == 1, nil
}

func writeCacheValues(store *api.Storage, campaignID, missionID string, values map[string]cacheValue) []CarriedCache {
	if len(values) == 0 || missionID == "" {
		return nil
	}
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sortStrings(keys)
	cat := category(campaignID)
	out := make([]CarriedCache, 0, len(keys))
	for _, key := range keys {
		v := values[key]
		dst := CarryCacheKey(missionID, key)
		switch v.kind {
		case "string":
			store.SetString(cat, dst, v.s)
		case "int":
			store.SetInt(cat, dst, v.i)
		case "real":
			store.SetReal(cat, dst, v.r)
		case "bool":
			store.SetBool(cat, dst, v.b)
		}
		out = append(out, CarriedCache{Key: key, Type: v.kind})
	}
	return out
}

func normalizeHookLog(lines, skipped []string) []string {
	out := make([]string, 0, len(lines)+len(skipped))
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line != "" {
			out = append(out, line)
		}
	}
	out = append(out, skipped...)
	return out
}

func setOf(values []string) map[string]bool {
	out := make(map[string]bool, len(values))
	for _, v := range values {
		out[v] = true
	}
	return out
}

func sortStrings(values []string) {
	for i := 1; i < len(values); i++ {
		for j := i; j > 0 && values[j] < values[j-1]; j-- {
			values[j], values[j-1] = values[j-1], values[j]
		}
	}
}
