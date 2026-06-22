package locale

import (
	"fmt"
	"io/fs"
	"path"
	"regexp"
	"sort"

	"github.com/BurntSushi/toml"
)

type Key string

const (
	HUDResourceGold               Key = "hud.resource.gold"
	HUDResourceLumber             Key = "hud.resource.lumber"
	HUDResourceFood               Key = "hud.resource.food"
	HUDResourceUpkeep             Key = "hud.resource.upkeep"
	HUDVitalLife                  Key = "hud.vital.life"
	HUDVitalMana                  Key = "hud.vital.mana"
	HUDSelectionPrefix            Key = "hud.selection.prefix"
	HUDQueuePrefix                Key = "hud.queue.prefix"
	HUDGroupsPrefix               Key = "hud.groups.prefix"
	HUDMenuOKTrue                 Key = "hud.menu.ok_true"
	HUDMenuOKFalse                Key = "hud.menu.ok_false"
	HUDIdleWorker                 Key = "hud.widget.idle_worker"
	HUDMinimap                    Key = "hud.widget.minimap"
	HUDCommandFootman             Key = "hud.command.group.footman"
	HUDCommandBarracks            Key = "hud.command.group.barracks"
	HUDCommandMoveLabel           Key = "hud.command.move.label"
	HUDCommandMoveTooltip         Key = "hud.command.move.tooltip"
	HUDCommandStopLabel           Key = "hud.command.stop.label"
	HUDCommandStopTooltip         Key = "hud.command.stop.tooltip"
	HUDCommandHoldLabel           Key = "hud.command.hold.label"
	HUDCommandHoldTooltip         Key = "hud.command.hold.tooltip"
	HUDCommandAttackLabel         Key = "hud.command.attack.label"
	HUDCommandAttackTooltip       Key = "hud.command.attack.tooltip"
	HUDCommandPatrolLabel         Key = "hud.command.patrol.label"
	HUDCommandPatrolTooltip       Key = "hud.command.patrol.tooltip"
	HUDCommandDefendLabel         Key = "hud.command.defend.label"
	HUDCommandDefendTooltip       Key = "hud.command.defend.tooltip"
	HUDCommandTrainFootmanLabel   Key = "hud.command.train_footman.label"
	HUDCommandTrainFootmanTooltip Key = "hud.command.train_footman.tooltip"
	HUDCommandTrainArcherLabel    Key = "hud.command.train_archer.label"
	HUDCommandTrainArcherTooltip  Key = "hud.command.train_archer.tooltip"
	HUDCommandRallyLabel          Key = "hud.command.rally.label"
	HUDCommandRallyTooltip        Key = "hud.command.rally.tooltip"
	CampaignMenuCampaignSelect    Key = "campaign.menu.campaign_select"
	CampaignMenuMissionSelect     Key = "campaign.menu.mission_select"
	CampaignMenuCarryOver         Key = "campaign.menu.carry_over"
	CampaignMenuNoCarryOver       Key = "campaign.menu.no_carry_over"
	CampaignMenuLocked            Key = "campaign.menu.locked"
	CampaignMenuAvailable         Key = "campaign.menu.available"
	CampaignMenuComplete          Key = "campaign.menu.complete"
	CampaignMenuMissingArchive    Key = "campaign.menu.missing_archive"
	CampaignMenuLevel             Key = "campaign.menu.level"
	CampaignMenuItems             Key = "campaign.menu.items"
	CampaignMenuArchive           Key = "campaign.menu.archive"
	CampaignMenuRequires          Key = "campaign.menu.requires"
	CampaignMenuError             Key = "campaign.menu.error"
	CampaignMenuFaction           Key = "campaign.menu.faction"
	EditorTitle                   Key = "editor.title"
	EditorProjectUntitled         Key = "editor.project.untitled"
	EditorModeLabel               Key = "editor.mode.label"
	EditorModeTerrain             Key = "editor.mode.terrain"
	EditorModeObjects             Key = "editor.mode.objects"
	EditorModeMetadata            Key = "editor.mode.metadata"
	EditorDirtyClean              Key = "editor.dirty.clean"
	EditorDirtyUnsaved            Key = "editor.dirty.unsaved"
	EditorActionNew               Key = "editor.action.new"
	EditorActionOpen              Key = "editor.action.open"
	EditorActionSave              Key = "editor.action.save"
	EditorActionExport            Key = "editor.action.export"
	EditorActionPlaytest          Key = "editor.action.playtest"
	EditorStatusReady             Key = "editor.status.ready"
	EditorStatusProjectCreated    Key = "editor.status.project_created"
	EditorStatusProjectOpened     Key = "editor.status.project_opened"
	EditorErrorOpen               Key = "editor.error.open"
	EditorConfirmNewTitle         Key = "editor.confirm.new.title"
	EditorConfirmNewBody          Key = "editor.confirm.new.body"
	EditorConfirmCancel           Key = "editor.confirm.cancel"
	EditorConfirmProceed          Key = "editor.confirm.proceed"
	EditorPanelTerrain            Key = "editor.panel.terrain"
	EditorPanelObjects            Key = "editor.panel.objects"
	EditorPanelMetadata           Key = "editor.panel.metadata"
	EditorPanelMinimap            Key = "editor.panel.minimap"
	EditorHintTerrain             Key = "editor.hint.terrain"
	EditorHintObjects             Key = "editor.hint.objects"
	EditorHintMetadata            Key = "editor.hint.metadata"
	EditorStatusPrefix            Key = "editor.status.prefix"
	EditorFieldCell               Key = "editor.field.cell"
	EditorFieldEntities           Key = "editor.field.entities"
	EditorFieldDoodads            Key = "editor.field.doodads"
	EditorFieldPalette            Key = "editor.field.palette"
	EditorFieldSelection          Key = "editor.field.selection"
	EditorFieldOverride           Key = "editor.field.override"
	EditorFieldBrush              Key = "editor.field.brush"
	EditorFieldCliff              Key = "editor.field.cliff"
	EditorFieldSplat              Key = "editor.field.splat"
	EditorFieldTool               Key = "editor.field.tool"
	EditorFieldPaint              Key = "editor.field.paint"
	EditorFieldFlags              Key = "editor.field.flags"
	EditorFieldID                 Key = "editor.field.id"
	EditorFieldName               Key = "editor.field.name"
	EditorFieldDescription        Key = "editor.field.description"
	EditorFieldEngine             Key = "editor.field.engine"
	EditorFieldPlayers            Key = "editor.field.players"
	EditorFieldTileset            Key = "editor.field.tileset"
	EditorFieldSplatSet           Key = "editor.field.splat_set"
	EditorFieldStarts             Key = "editor.field.starts"
	EditorFieldSeedPolicy         Key = "editor.field.seed_policy"
	EditorFieldPath               Key = "editor.field.path"
	EditorFieldCamera             Key = "editor.field.camera"
	EditorScopeNoTriggerGUI       Key = "editor.scope.no_trigger_gui"
)

var requiredKeys = []string{
	string(HUDResourceGold),
	string(HUDResourceLumber),
	string(HUDResourceFood),
	string(HUDResourceUpkeep),
	string(HUDVitalLife),
	string(HUDVitalMana),
	string(HUDSelectionPrefix),
	string(HUDQueuePrefix),
	string(HUDGroupsPrefix),
	string(HUDMenuOKTrue),
	string(HUDMenuOKFalse),
	string(HUDIdleWorker),
	string(HUDMinimap),
	string(HUDCommandFootman),
	string(HUDCommandBarracks),
	string(HUDCommandMoveLabel),
	string(HUDCommandMoveTooltip),
	string(HUDCommandStopLabel),
	string(HUDCommandStopTooltip),
	string(HUDCommandHoldLabel),
	string(HUDCommandHoldTooltip),
	string(HUDCommandAttackLabel),
	string(HUDCommandAttackTooltip),
	string(HUDCommandPatrolLabel),
	string(HUDCommandPatrolTooltip),
	string(HUDCommandDefendLabel),
	string(HUDCommandDefendTooltip),
	string(HUDCommandTrainFootmanLabel),
	string(HUDCommandTrainFootmanTooltip),
	string(HUDCommandTrainArcherLabel),
	string(HUDCommandTrainArcherTooltip),
	string(HUDCommandRallyLabel),
	string(HUDCommandRallyTooltip),
	string(CampaignMenuCampaignSelect),
	string(CampaignMenuMissionSelect),
	string(CampaignMenuCarryOver),
	string(CampaignMenuNoCarryOver),
	string(CampaignMenuLocked),
	string(CampaignMenuAvailable),
	string(CampaignMenuComplete),
	string(CampaignMenuMissingArchive),
	string(CampaignMenuLevel),
	string(CampaignMenuItems),
	string(CampaignMenuArchive),
	string(CampaignMenuRequires),
	string(CampaignMenuError),
	string(CampaignMenuFaction),
	string(EditorTitle),
	string(EditorProjectUntitled),
	string(EditorModeLabel),
	string(EditorModeTerrain),
	string(EditorModeObjects),
	string(EditorModeMetadata),
	string(EditorDirtyClean),
	string(EditorDirtyUnsaved),
	string(EditorActionNew),
	string(EditorActionOpen),
	string(EditorActionSave),
	string(EditorActionExport),
	string(EditorActionPlaytest),
	string(EditorStatusReady),
	string(EditorStatusProjectCreated),
	string(EditorStatusProjectOpened),
	string(EditorErrorOpen),
	string(EditorConfirmNewTitle),
	string(EditorConfirmNewBody),
	string(EditorConfirmCancel),
	string(EditorConfirmProceed),
	string(EditorPanelTerrain),
	string(EditorPanelObjects),
	string(EditorPanelMetadata),
	string(EditorPanelMinimap),
	string(EditorHintTerrain),
	string(EditorHintObjects),
	string(EditorHintMetadata),
	string(EditorStatusPrefix),
	string(EditorFieldCell),
	string(EditorFieldEntities),
	string(EditorFieldDoodads),
	string(EditorFieldPalette),
	string(EditorFieldSelection),
	string(EditorFieldOverride),
	string(EditorFieldBrush),
	string(EditorFieldCliff),
	string(EditorFieldSplat),
	string(EditorFieldTool),
	string(EditorFieldPaint),
	string(EditorFieldFlags),
	string(EditorFieldID),
	string(EditorFieldName),
	string(EditorFieldDescription),
	string(EditorFieldEngine),
	string(EditorFieldPlayers),
	string(EditorFieldTileset),
	string(EditorFieldSplatSet),
	string(EditorFieldStarts),
	string(EditorFieldSeedPolicy),
	string(EditorFieldPath),
	string(EditorFieldCamera),
	string(EditorScopeNoTriggerGUI),
}

var tagPattern = regexp.MustCompile(`^[a-z][a-z0-9_-]{1,15}$`)

type Table struct {
	Tag     string
	Strings map[string]string
}

type Violation struct {
	Path string
	Rule string
	Msg  string
}

func (v Violation) String() string { return v.Path + ": " + v.Rule + ": " + v.Msg }

func RequiredKeys() []string {
	out := append([]string{}, requiredKeys...)
	sort.Strings(out)
	return out
}

func Load(fsys fs.FS, tag string) (*Table, error) {
	t, err := Read(fsys, tag)
	if err != nil {
		return nil, err
	}
	file := path.Join("locale", tag+".toml")
	if violations := ValidateTable(file, t, RequiredKeys()); len(violations) > 0 {
		return nil, fmt.Errorf("locale: %s", violations[0].String())
	}
	return t, nil
}

func Read(fsys fs.FS, tag string) (*Table, error) {
	if !tagPattern.MatchString(tag) {
		return nil, fmt.Errorf("locale: invalid tag %q", tag)
	}
	file := path.Join("locale", tag+".toml")
	blob, err := fs.ReadFile(fsys, file)
	if err != nil {
		return nil, fmt.Errorf("locale: read %s: %w", file, err)
	}
	var raw struct {
		Strings map[string]string `toml:"strings"`
	}
	md, err := toml.Decode(string(blob), &raw)
	if err != nil {
		return nil, fmt.Errorf("locale: %s: %w", file, err)
	}
	for _, un := range md.Undecoded() {
		return nil, fmt.Errorf("locale: %s: unknown field %q", file, un.String())
	}
	if len(raw.Strings) == 0 {
		return nil, fmt.Errorf("locale: %s: [strings] must not be empty", file)
	}
	return &Table{Tag: tag, Strings: raw.Strings}, nil
}

func ValidateTable(file string, t *Table, required []string) []Violation {
	requiredSet := map[string]bool{}
	var violations []Violation
	for _, key := range required {
		requiredSet[key] = true
		if t == nil || t.Strings[key] == "" {
			violations = append(violations, Violation{Path: file, Rule: "LOCALE-MISSING", Msg: fmt.Sprintf("missing required key %q", key)})
		}
	}
	if t == nil {
		return violations
	}
	for key := range t.Strings {
		if !requiredSet[key] {
			violations = append(violations, Violation{Path: file, Rule: "LOCALE-UNUSED", Msg: fmt.Sprintf("unused locale key %q", key)})
		}
	}
	sort.Slice(violations, func(i, j int) bool {
		if violations[i].Rule != violations[j].Rule {
			return violations[i].Rule < violations[j].Rule
		}
		return violations[i].Msg < violations[j].Msg
	})
	return violations
}

func (t *Table) Lookup(key Key) (string, bool) {
	if t == nil {
		return "", false
	}
	value, ok := t.Strings[string(key)]
	return value, ok
}

func (t *Table) Must(key Key) string {
	value, ok := t.Lookup(key)
	if !ok || value == "" {
		panic("locale: missing key " + string(key))
	}
	return value
}
