package campaign

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io/fs"
	"path"
	"regexp"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

const (
	MaxCarryHeroes = 8
	MaxCarryItems  = 6
)

var idPattern = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{0,63}$`)

type Definition struct {
	ID       string
	Title    string
	Faction  string
	Missions []Mission
}

type Mission struct {
	ID       string
	Title    string
	Summary  string
	Archive  string
	Requires []string
}

type MissionStatus string

const (
	StatusLocked         MissionStatus = "locked"
	StatusAvailable      MissionStatus = "available"
	StatusComplete       MissionStatus = "complete"
	StatusMissingArchive MissionStatus = "missing-archive"
)

type HeroCarryOver struct {
	Name  string   `json:"name"`
	Level int      `json:"level"`
	Items []string `json:"items,omitempty"`
}

type CarryOver struct {
	MissionID string          `json:"missionId"`
	Heroes    []HeroCarryOver `json:"heroes,omitempty"`
}

type MissionView struct {
	ID        string        `json:"id"`
	Title     string        `json:"title"`
	Summary   string        `json:"summary,omitempty"`
	Archive   string        `json:"archive"`
	Requires  []string      `json:"requires,omitempty"`
	Status    MissionStatus `json:"status"`
	Unlocked  bool          `json:"unlocked"`
	Complete  bool          `json:"complete"`
	Error     string        `json:"error,omitempty"`
	CarryOver CarryOver     `json:"carryOver"`
}

type View struct {
	CampaignID        string        `json:"campaignId"`
	Title             string        `json:"title"`
	Faction           string        `json:"faction"`
	SelectedMissionID string        `json:"selectedMissionId"`
	SelectedIndex     int           `json:"selectedIndex"`
	Missions          []MissionView `json:"missions"`
	CarryOver         CarryOver     `json:"carryOver"`
}

type CatalogChoice struct {
	ID                string `json:"id"`
	Title             string `json:"title"`
	Faction           string `json:"faction"`
	Missions          int    `json:"missions"`
	CompletedMissions int    `json:"completedMissions"`
}

type CatalogView struct {
	SelectedCampaignID string          `json:"selectedCampaignId"`
	Campaigns          []CatalogChoice `json:"campaigns"`
}

type StoreSnapshot struct {
	Category string                 `json:"category"`
	Missions []MissionStoreSnapshot `json:"missions"`
	Carry    []CarryOver            `json:"carry"`
	Bytes    int                    `json:"bytes"`
	SHA256   string                 `json:"sha256"`
}

type MissionStoreSnapshot struct {
	ID              string `json:"id"`
	CompletePresent bool   `json:"completePresent"`
	Complete        bool   `json:"complete"`
}

type rawDefinition struct {
	ID      string       `toml:"id"`
	Title   string       `toml:"title"`
	Faction string       `toml:"faction"`
	Mission []rawMission `toml:"mission"`
}

type rawMission struct {
	ID       string   `toml:"id"`
	Title    string   `toml:"title"`
	Summary  string   `toml:"summary"`
	Archive  string   `toml:"archive"`
	Requires []string `toml:"requires"`
}

func Load(fsys fs.FS, file string) (Definition, error) {
	blob, err := fs.ReadFile(fsys, file)
	if err != nil {
		return Definition{}, fmt.Errorf("campaign: read %s: %w", file, err)
	}
	return ReadDefinition(file, blob)
}

func LoadCatalog(fsys fs.FS, dir string) ([]Definition, error) {
	entries, err := fs.ReadDir(fsys, dir)
	if err != nil {
		return nil, fmt.Errorf("campaign: read catalog %s: %w", dir, err)
	}
	var defs []Definition
	for _, entry := range entries {
		if entry.IsDir() || path.Ext(entry.Name()) != ".toml" {
			continue
		}
		def, err := Load(fsys, path.Join(dir, entry.Name()))
		if err != nil {
			return nil, err
		}
		defs = append(defs, def)
	}
	if len(defs) == 0 {
		return nil, fmt.Errorf("campaign: catalog %s has no .toml definitions", dir)
	}
	sort.Slice(defs, func(i, j int) bool {
		if defs[i].Title != defs[j].Title {
			return defs[i].Title < defs[j].Title
		}
		return defs[i].ID < defs[j].ID
	})
	return defs, nil
}

func ReadDefinition(name string, blob []byte) (Definition, error) {
	var raw rawDefinition
	md, err := toml.Decode(string(blob), &raw)
	if err != nil {
		return Definition{}, fmt.Errorf("campaign: %s: %w", name, err)
	}
	if undecoded := md.Undecoded(); len(undecoded) > 0 {
		return Definition{}, fmt.Errorf("campaign: %s: unknown field %q", name, undecoded[0].String())
	}
	def := Definition{
		ID:       strings.TrimSpace(raw.ID),
		Title:    strings.TrimSpace(raw.Title),
		Faction:  strings.TrimSpace(raw.Faction),
		Missions: make([]Mission, 0, len(raw.Mission)),
	}
	for _, m := range raw.Mission {
		def.Missions = append(def.Missions, Mission{
			ID:       strings.TrimSpace(m.ID),
			Title:    strings.TrimSpace(m.Title),
			Summary:  strings.TrimSpace(m.Summary),
			Archive:  strings.TrimSpace(m.Archive),
			Requires: trimStrings(m.Requires),
		})
	}
	if err := Validate(def); err != nil {
		return Definition{}, fmt.Errorf("campaign: %s: %w", name, err)
	}
	return def, nil
}

func Validate(def Definition) error {
	if !idPattern.MatchString(def.ID) {
		return fmt.Errorf("campaign id %q must match %s", def.ID, idPattern.String())
	}
	if strings.TrimSpace(def.Title) == "" {
		return fmt.Errorf("campaign %s title is required", def.ID)
	}
	if strings.TrimSpace(def.Faction) == "" {
		return fmt.Errorf("campaign %s faction is required", def.ID)
	}
	if len(def.Missions) == 0 {
		return fmt.Errorf("campaign %s must define at least one mission", def.ID)
	}

	seen := map[string]int{}
	for i, m := range def.Missions {
		if !idPattern.MatchString(m.ID) {
			return fmt.Errorf("mission[%d] id %q must match %s", i, m.ID, idPattern.String())
		}
		if _, ok := seen[m.ID]; ok {
			return fmt.Errorf("mission %q is duplicated", m.ID)
		}
		seen[m.ID] = i
		if strings.TrimSpace(m.Title) == "" {
			return fmt.Errorf("mission %s title is required", m.ID)
		}
		if err := validateArchivePath(m.Archive); err != nil {
			return fmt.Errorf("mission %s archive: %w", m.ID, err)
		}
	}
	for _, m := range def.Missions {
		for _, req := range m.Requires {
			if req == m.ID {
				return fmt.Errorf("mission %s cannot require itself", m.ID)
			}
			if _, ok := seen[req]; !ok {
				return fmt.Errorf("mission %s requires unknown mission %s", m.ID, req)
			}
		}
	}
	if err := detectRequirementCycle(def); err != nil {
		return err
	}
	return nil
}

func BuildCatalogView(defs []Definition, store *api.Storage, selectedID string) (CatalogView, error) {
	if store == nil {
		return CatalogView{}, fmt.Errorf("campaign: nil storage")
	}
	if len(defs) == 0 {
		return CatalogView{}, fmt.Errorf("campaign: catalog view requires at least one definition")
	}
	selectedID = strings.TrimSpace(selectedID)
	out := CatalogView{Campaigns: make([]CatalogChoice, 0, len(defs))}
	seen := map[string]bool{}
	for _, def := range defs {
		if err := Validate(def); err != nil {
			return CatalogView{}, err
		}
		if seen[def.ID] {
			return CatalogView{}, fmt.Errorf("campaign: duplicate catalog campaign %s", def.ID)
		}
		seen[def.ID] = true
		if out.SelectedCampaignID == "" {
			out.SelectedCampaignID = def.ID
		}
		if selectedID == def.ID {
			out.SelectedCampaignID = def.ID
		}
		out.Campaigns = append(out.Campaigns, CatalogChoice{
			ID:                def.ID,
			Title:             def.Title,
			Faction:           def.Faction,
			Missions:          len(def.Missions),
			CompletedMissions: completedCount(def, store),
		})
	}
	if selectedID != "" && out.SelectedCampaignID != selectedID {
		return CatalogView{}, fmt.Errorf("campaign: selected campaign %q is not in catalog", selectedID)
	}
	return out, nil
}

func BuildMissionView(def Definition, store *api.Storage, archives fs.FS, selectedMissionID string) (View, error) {
	if err := Validate(def); err != nil {
		return View{}, err
	}
	if store == nil {
		return View{}, fmt.Errorf("campaign: nil storage")
	}
	if archives == nil {
		return View{}, fmt.Errorf("campaign: nil archive filesystem")
	}
	selectedMissionID = strings.TrimSpace(selectedMissionID)

	completed := map[string]bool{}
	for _, m := range def.Missions {
		completed[m.ID], _ = store.GetBool(category(def.ID), completeKey(m.ID))
	}

	out := View{
		CampaignID: def.ID,
		Title:      def.Title,
		Faction:    def.Faction,
		Missions:   make([]MissionView, 0, len(def.Missions)),
	}
	selectedIndex := -1
	for i, m := range def.Missions {
		complete := completed[m.ID]
		unlocked := complete || missionUnlocked(m, completed)
		status := StatusLocked
		errText := ""
		if complete {
			status = StatusComplete
		} else if unlocked {
			status = StatusAvailable
			if _, err := fs.Stat(archives, m.Archive); err != nil {
				status = StatusMissingArchive
				errText = fmt.Sprintf("missing archive %s: %v", m.Archive, err)
			}
		}
		carry, err := readCarryOver(store, def.ID, m.ID)
		if err != nil {
			return View{}, err
		}
		mv := MissionView{
			ID:        m.ID,
			Title:     m.Title,
			Summary:   m.Summary,
			Archive:   m.Archive,
			Requires:  append([]string{}, m.Requires...),
			Status:    status,
			Unlocked:  unlocked,
			Complete:  complete,
			Error:     errText,
			CarryOver: carry,
		}
		out.Missions = append(out.Missions, mv)
		if selectedMissionID != "" && selectedMissionID == m.ID {
			selectedIndex = i
		}
	}
	if selectedMissionID != "" && selectedIndex < 0 {
		return View{}, fmt.Errorf("campaign: selected mission %q is not in campaign %s", selectedMissionID, def.ID)
	}
	if selectedIndex < 0 {
		selectedIndex = defaultSelectedMission(out.Missions)
	}
	out.SelectedIndex = selectedIndex
	out.SelectedMissionID = out.Missions[selectedIndex].ID
	out.CarryOver = out.Missions[selectedIndex].CarryOver
	return out, nil
}

func CompleteMission(store *api.Storage, def Definition, missionID string, carry CarryOver) error {
	if err := Validate(def); err != nil {
		return err
	}
	if store == nil {
		return fmt.Errorf("campaign: nil storage")
	}
	missionID = strings.TrimSpace(missionID)
	if !missionExists(def, missionID) {
		return fmt.Errorf("campaign: mission %q is not in campaign %s", missionID, def.ID)
	}
	if strings.TrimSpace(carry.MissionID) == "" {
		store.SetBool(category(def.ID), completeKey(missionID), true)
		return nil
	}
	var err error
	carry, err = normalizeCarryOver(def, carry)
	if err != nil {
		return err
	}
	store.SetBool(category(def.ID), completeKey(missionID), true)
	return writeCarryOver(store, def, carry)
}

func SnapshotStore(def Definition, store *api.Storage) (StoreSnapshot, error) {
	if err := Validate(def); err != nil {
		return StoreSnapshot{}, err
	}
	if store == nil {
		return StoreSnapshot{}, fmt.Errorf("campaign: nil storage")
	}
	var buf bytes.Buffer
	if err := store.Save(&buf); err != nil {
		return StoreSnapshot{}, err
	}
	sum := sha256.Sum256(buf.Bytes())
	out := StoreSnapshot{
		Category: category(def.ID),
		Missions: make([]MissionStoreSnapshot, 0, len(def.Missions)),
		Bytes:    buf.Len(),
		SHA256:   hex.EncodeToString(sum[:]),
	}
	for _, m := range def.Missions {
		complete, ok := store.GetBool(category(def.ID), completeKey(m.ID))
		out.Missions = append(out.Missions, MissionStoreSnapshot{ID: m.ID, CompletePresent: ok, Complete: complete})
		carry, err := readCarryOver(store, def.ID, m.ID)
		if err != nil {
			return StoreSnapshot{}, err
		}
		if len(carry.Heroes) > 0 {
			out.Carry = append(out.Carry, carry)
		}
	}
	return out, nil
}

func writeCarryOver(store *api.Storage, def Definition, carry CarryOver) error {
	var err error
	carry, err = normalizeCarryOver(def, carry)
	if err != nil {
		return err
	}
	cat := category(def.ID)
	store.SetInt(cat, carryHeroCountKey(carry.MissionID), len(carry.Heroes))
	for i, h := range carry.Heroes {
		store.SetString(cat, carryHeroNameKey(carry.MissionID, i), h.Name)
		store.SetInt(cat, carryHeroLevelKey(carry.MissionID, i), h.Level)
		store.SetInt(cat, carryHeroItemCountKey(carry.MissionID, i), len(h.Items))
		for j, item := range h.Items {
			store.SetString(cat, carryHeroItemKey(carry.MissionID, i, j), item)
		}
	}
	return nil
}

func normalizeCarryOver(def Definition, carry CarryOver) (CarryOver, error) {
	carry.MissionID = strings.TrimSpace(carry.MissionID)
	if !missionExists(def, carry.MissionID) {
		return CarryOver{}, fmt.Errorf("campaign: carry-over target mission %q is not in campaign %s", carry.MissionID, def.ID)
	}
	if len(carry.Heroes) > MaxCarryHeroes {
		return CarryOver{}, fmt.Errorf("campaign: carry-over target %s has %d heroes, max %d", carry.MissionID, len(carry.Heroes), MaxCarryHeroes)
	}
	for i, h := range carry.Heroes {
		name := strings.TrimSpace(h.Name)
		if name == "" {
			return CarryOver{}, fmt.Errorf("campaign: carry-over hero %d for %s has empty name", i, carry.MissionID)
		}
		if h.Level < 1 {
			return CarryOver{}, fmt.Errorf("campaign: carry-over hero %s level %d must be >= 1", name, h.Level)
		}
		if len(h.Items) > MaxCarryItems {
			return CarryOver{}, fmt.Errorf("campaign: carry-over hero %s has %d items, max %d", name, len(h.Items), MaxCarryItems)
		}
		carry.Heroes[i].Name = name
		for j, item := range h.Items {
			item = strings.TrimSpace(item)
			if item == "" {
				return CarryOver{}, fmt.Errorf("campaign: carry-over hero %s item %d is empty", name, j)
			}
			carry.Heroes[i].Items[j] = item
		}
	}
	return carry, nil
}

func readCarryOver(store *api.Storage, campaignID, missionID string) (CarryOver, error) {
	carry := CarryOver{MissionID: missionID}
	count, ok := store.GetInt(category(campaignID), carryHeroCountKey(missionID))
	if !ok || count == 0 {
		return carry, nil
	}
	if count < 0 || count > MaxCarryHeroes {
		return CarryOver{}, fmt.Errorf("campaign: carry-over %s hero-count %d outside [0,%d]", missionID, count, MaxCarryHeroes)
	}
	carry.Heroes = make([]HeroCarryOver, 0, count)
	for i := 0; i < count; i++ {
		name, ok := store.GetString(category(campaignID), carryHeroNameKey(missionID, i))
		if !ok || strings.TrimSpace(name) == "" {
			return CarryOver{}, fmt.Errorf("campaign: carry-over %s hero %d missing name", missionID, i)
		}
		level, ok := store.GetInt(category(campaignID), carryHeroLevelKey(missionID, i))
		if !ok || level < 1 {
			return CarryOver{}, fmt.Errorf("campaign: carry-over %s hero %s invalid level %d", missionID, name, level)
		}
		itemCount, ok := store.GetInt(category(campaignID), carryHeroItemCountKey(missionID, i))
		if !ok {
			itemCount = 0
		}
		if itemCount < 0 || itemCount > MaxCarryItems {
			return CarryOver{}, fmt.Errorf("campaign: carry-over %s hero %s item-count %d outside [0,%d]", missionID, name, itemCount, MaxCarryItems)
		}
		hero := HeroCarryOver{Name: name, Level: level, Items: make([]string, 0, itemCount)}
		for j := 0; j < itemCount; j++ {
			item, ok := store.GetString(category(campaignID), carryHeroItemKey(missionID, i, j))
			if !ok || strings.TrimSpace(item) == "" {
				return CarryOver{}, fmt.Errorf("campaign: carry-over %s hero %s missing item %d", missionID, name, j)
			}
			hero.Items = append(hero.Items, item)
		}
		carry.Heroes = append(carry.Heroes, hero)
	}
	return carry, nil
}

func defaultSelectedMission(missions []MissionView) int {
	for i, m := range missions {
		if m.Status == StatusAvailable || m.Status == StatusMissingArchive {
			return i
		}
	}
	for i, m := range missions {
		if !m.Complete {
			return i
		}
	}
	return 0
}

func missionUnlocked(m Mission, completed map[string]bool) bool {
	for _, req := range m.Requires {
		if !completed[req] {
			return false
		}
	}
	return true
}

func completedCount(def Definition, store *api.Storage) int {
	total := 0
	for _, m := range def.Missions {
		if complete, _ := store.GetBool(category(def.ID), completeKey(m.ID)); complete {
			total++
		}
	}
	return total
}

func missionExists(def Definition, id string) bool {
	for _, m := range def.Missions {
		if m.ID == id {
			return true
		}
	}
	return false
}

func validateArchivePath(p string) error {
	if strings.TrimSpace(p) == "" {
		return fmt.Errorf("path is required")
	}
	if path.IsAbs(p) {
		return fmt.Errorf("%q must be relative", p)
	}
	clean := path.Clean(p)
	if clean == "." || clean != p || strings.HasPrefix(clean, "../") || clean == ".." {
		return fmt.Errorf("%q must be a clean relative path", p)
	}
	return nil
}

func detectRequirementCycle(def Definition) error {
	byID := map[string]Mission{}
	for _, m := range def.Missions {
		byID[m.ID] = m
	}
	visiting := map[string]bool{}
	visited := map[string]bool{}
	var visit func(string) error
	visit = func(id string) error {
		if visiting[id] {
			return fmt.Errorf("mission requirements contain a cycle at %s", id)
		}
		if visited[id] {
			return nil
		}
		visiting[id] = true
		for _, req := range byID[id].Requires {
			if err := visit(req); err != nil {
				return err
			}
		}
		visiting[id] = false
		visited[id] = true
		return nil
	}
	for _, m := range def.Missions {
		if err := visit(m.ID); err != nil {
			return err
		}
	}
	return nil
}

func trimStrings(in []string) []string {
	out := make([]string, 0, len(in))
	for _, s := range in {
		s = strings.TrimSpace(s)
		if s != "" {
			out = append(out, s)
		}
	}
	return out
}

func category(campaignID string) string { return "campaign:" + campaignID }

func completeKey(missionID string) string { return "mission:" + missionID + ":complete" }

func carryHeroCountKey(missionID string) string {
	return "carry:" + missionID + ":hero-count"
}

func carryHeroNameKey(missionID string, i int) string {
	return fmt.Sprintf("carry:%s:hero:%d:name", missionID, i)
}

func carryHeroLevelKey(missionID string, i int) string {
	return fmt.Sprintf("carry:%s:hero:%d:level", missionID, i)
}

func carryHeroItemCountKey(missionID string, i int) string {
	return fmt.Sprintf("carry:%s:hero:%d:item-count", missionID, i)
}

func carryHeroItemKey(missionID string, i, j int) string {
	return fmt.Sprintf("carry:%s:hero:%d:item:%d", missionID, i, j)
}
