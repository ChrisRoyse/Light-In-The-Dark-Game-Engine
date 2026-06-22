package hud

import (
	"fmt"
	"strconv"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
)

type CampaignMenuScreen string

const (
	CampaignMenuScreenCampaignSelect CampaignMenuScreen = "campaign-select"
	CampaignMenuScreenMissionSelect  CampaignMenuScreen = "mission-select"
)

type CampaignMenuStrings struct {
	CampaignSelect string
	MissionSelect  string
	CarryOver      string
	NoCarryOver    string
	Locked         string
	Available      string
	Complete       string
	MissingArchive string
	Level          string
	Items          string
	Archive        string
	Requires       string
	Error          string
	Faction        string
}

type CampaignMenuLabel struct {
	Name   string `json:"name"`
	Parent string `json:"parent"`
	Text   string `json:"text"`
	Rect   Rect   `json:"rect"`
}

type CampaignMenuLayout struct {
	Screen             CampaignMenuScreen    `json:"screen"`
	Canvas             Canvas                `json:"canvas"`
	Widgets            []Widget              `json:"widgets"`
	Labels             []CampaignMenuLabel   `json:"labels"`
	SelectedCampaignID string                `json:"selectedCampaignId,omitempty"`
	SelectedMissionID  string                `json:"selectedMissionId,omitempty"`
	Catalog            *campaign.CatalogView `json:"catalog,omitempty"`
	Mission            *campaign.View        `json:"mission,omitempty"`
	ExpectedDrawCalls  int                   `json:"expectedDrawCalls"`
	Issues             []LayoutIssue         `json:"issues,omitempty"`
}

func CampaignMenuStringsFromLocale(table *locale.Table) CampaignMenuStrings {
	return CampaignMenuStrings{
		CampaignSelect: table.Must(locale.CampaignMenuCampaignSelect),
		MissionSelect:  table.Must(locale.CampaignMenuMissionSelect),
		CarryOver:      table.Must(locale.CampaignMenuCarryOver),
		NoCarryOver:    table.Must(locale.CampaignMenuNoCarryOver),
		Locked:         table.Must(locale.CampaignMenuLocked),
		Available:      table.Must(locale.CampaignMenuAvailable),
		Complete:       table.Must(locale.CampaignMenuComplete),
		MissingArchive: table.Must(locale.CampaignMenuMissingArchive),
		Level:          table.Must(locale.CampaignMenuLevel),
		Items:          table.Must(locale.CampaignMenuItems),
		Archive:        table.Must(locale.CampaignMenuArchive),
		Requires:       table.Must(locale.CampaignMenuRequires),
		Error:          table.Must(locale.CampaignMenuError),
		Faction:        table.Must(locale.CampaignMenuFaction),
	}
}

func NewCampaignSelectLayout(canvas Canvas, view campaign.CatalogView, labels CampaignMenuStrings) CampaignMenuLayout {
	widgets := campaignMenuWidgets(canvas, CampaignMenuScreenCampaignSelect)
	layout := CampaignMenuLayout{
		Screen:             CampaignMenuScreenCampaignSelect,
		Canvas:             canvas,
		Widgets:            widgets,
		SelectedCampaignID: view.SelectedCampaignID,
		Catalog:            &view,
	}
	add := campaignLabelAdder(canvas, &layout)
	header := mustWidget(widgets, "campaign-header")
	list := mustWidget(widgets, "campaign-list")
	detail := mustWidget(widgets, "campaign-detail")
	add("campaign-title", header, labels.CampaignSelect, 14, 7, 900, 24)
	for i, c := range view.Campaigns {
		line := c.Title
		if c.ID == view.SelectedCampaignID {
			line = ">" + " " + line
		}
		line = fmt.Sprintf("%s  %s %s  %d/%d", line, labels.Faction, c.Faction, c.CompletedMissions, c.Missions)
		add("campaign-"+c.ID, list, line, 18, 22+float64(i*34), 320, 24)
	}
	for _, c := range view.Campaigns {
		if c.ID != view.SelectedCampaignID {
			continue
		}
		add("campaign-detail-title", detail, c.Title, 20, 24, 700, 28)
		add("campaign-detail-faction", detail, labels.Faction+" "+c.Faction, 20, 68, 620, 24)
		add("campaign-detail-progress", detail, fmt.Sprintf("%d/%d", c.CompletedMissions, c.Missions), 20, 104, 180, 24)
		break
	}
	finalizeCampaignMenuLayout(&layout)
	return layout
}

func NewMissionSelectLayout(canvas Canvas, view campaign.View, labels CampaignMenuStrings) CampaignMenuLayout {
	widgets := campaignMenuWidgets(canvas, CampaignMenuScreenMissionSelect)
	layout := CampaignMenuLayout{
		Screen:             CampaignMenuScreenMissionSelect,
		Canvas:             canvas,
		Widgets:            widgets,
		SelectedCampaignID: view.CampaignID,
		SelectedMissionID:  view.SelectedMissionID,
		Mission:            &view,
	}
	add := campaignLabelAdder(canvas, &layout)
	header := mustWidget(widgets, "campaign-header")
	list := mustWidget(widgets, "mission-list")
	detail := mustWidget(widgets, "mission-detail")
	carry := mustWidget(widgets, "carry-over")
	add("mission-title", header, labels.MissionSelect+" - "+view.Title, 14, 7, 960, 24)
	for i, m := range view.Missions {
		line := statusLabel(labels, m.Status) + "  " + m.Title
		if m.ID == view.SelectedMissionID {
			line = ">" + " " + line
		}
		add("mission-"+m.ID, list, line, 18, 22+float64(i*34), 360, 24)
	}
	selected := view.Missions[view.SelectedIndex]
	add("mission-detail-title", detail, selected.Title, 20, 22, 660, 26)
	add("mission-detail-status", detail, statusLabel(labels, selected.Status), 20, 58, 280, 24)
	add("mission-detail-archive", detail, labels.Archive+" "+selected.Archive, 20, 92, 660, 24)
	if len(selected.Requires) > 0 {
		add("mission-detail-requires", detail, labels.Requires+" "+strings.Join(selected.Requires, ", "), 20, 126, 660, 24)
	}
	if selected.Summary != "" {
		add("mission-detail-summary", detail, selected.Summary, 20, 164, 660, 24)
	}
	if selected.Error != "" {
		add("mission-detail-error", detail, labels.Error+" "+selected.Error, 20, 198, 680, 24)
	}
	add("carry-title", carry, labels.CarryOver, 20, 22, 420, 26)
	if len(view.CarryOver.Heroes) == 0 {
		add("carry-empty", carry, labels.NoCarryOver, 20, 62, 420, 24)
	} else {
		y := 62.0
		for i, h := range view.CarryOver.Heroes {
			add("carry-hero-"+strconv.Itoa(i), carry, h.Name+"  "+labels.Level+" "+strconv.Itoa(h.Level), 20, y, 640, 24)
			y += 32
			if len(h.Items) > 0 {
				add("carry-hero-"+strconv.Itoa(i)+"-items", carry, labels.Items+" "+strings.Join(h.Items, ", "), 42, y, 620, 24)
				y += 32
			}
		}
	}
	finalizeCampaignMenuLayout(&layout)
	return layout
}

func statusLabel(labels CampaignMenuStrings, status campaign.MissionStatus) string {
	switch status {
	case campaign.StatusLocked:
		return labels.Locked
	case campaign.StatusAvailable:
		return labels.Available
	case campaign.StatusComplete:
		return labels.Complete
	case campaign.StatusMissingArchive:
		return labels.MissingArchive
	default:
		return string(status)
	}
}

func campaignMenuWidgets(canvas Canvas, screen CampaignMenuScreen) []Widget {
	specs := []WidgetSpec{
		{Name: "campaign-header", Kind: WidgetNineSlice, Anchor: AnchorTop, Ref: RefRect{X: 48, Y: 28, W: 1184, H: 38}, AtlasRegion: "panel-small"},
	}
	if screen == CampaignMenuScreenCampaignSelect {
		specs = append(specs,
			WidgetSpec{Name: "campaign-list", Kind: WidgetNineSlice, Anchor: AnchorTopLeft, Ref: RefRect{X: 48, Y: 84, W: 360, H: 540}, AtlasRegion: "panel-large"},
			WidgetSpec{Name: "campaign-detail", Kind: WidgetNineSlice, Anchor: AnchorTopRight, Ref: RefRect{X: 432, Y: 84, W: 800, H: 540}, AtlasRegion: "panel-large"},
		)
	} else {
		specs = append(specs,
			WidgetSpec{Name: "mission-list", Kind: WidgetNineSlice, Anchor: AnchorTopLeft, Ref: RefRect{X: 48, Y: 84, W: 420, H: 540}, AtlasRegion: "panel-large"},
			WidgetSpec{Name: "mission-detail", Kind: WidgetNineSlice, Anchor: AnchorTopRight, Ref: RefRect{X: 492, Y: 84, W: 740, H: 250}, AtlasRegion: "panel-large"},
			WidgetSpec{Name: "carry-over", Kind: WidgetNineSlice, Anchor: AnchorTopRight, Ref: RefRect{X: 492, Y: 354, W: 740, H: 270}, AtlasRegion: "panel-large"},
		)
	}
	widgets := make([]Widget, 0, len(specs))
	for _, spec := range specs {
		widgets = append(widgets, Widget{WidgetSpec: spec, Rect: canvas.Place(spec.Anchor, spec.Ref)})
	}
	return widgets
}

func campaignLabelAdder(canvas Canvas, layout *CampaignMenuLayout) func(string, Widget, string, float64, float64, float64, float64) {
	return func(name string, parent Widget, text string, x, y, w, h float64) {
		layout.Labels = append(layout.Labels, CampaignMenuLabel{
			Name:   name,
			Parent: parent.Name,
			Text:   text,
			Rect: Rect{
				X: parent.Rect.X + canvas.Snap(x),
				Y: parent.Rect.Y + canvas.Snap(y),
				W: canvas.Snap(w),
				H: canvas.Snap(h),
			},
		})
	}
}

func finalizeCampaignMenuLayout(layout *CampaignMenuLayout) {
	layout.ExpectedDrawCalls = len(layout.Widgets) + len(layout.Labels)
	layout.Issues = ValidateCampaignMenuLayout(*layout)
}

func ValidateCampaignMenuLayout(layout CampaignMenuLayout) []LayoutIssue {
	issues := ValidateWidgets(layout.Widgets, layout.Canvas.Width, layout.Canvas.Height)
	for i, label := range layout.Labels {
		parent, ok := findWidget(layout.Widgets, label.Parent)
		if !ok {
			issues = append(issues, LayoutIssue{Widget: label.Name, Rule: "label-parent", Msg: "label parent is missing"})
			continue
		}
		if !label.Rect.InsideRect(parent.Rect) {
			issues = append(issues, LayoutIssue{Widget: label.Name, Rule: "label-offscreen", Msg: "label leaves parent widget"})
		}
		for j := 0; j < i; j++ {
			prev := layout.Labels[j]
			if prev.Parent == label.Parent && label.Rect.Overlaps(prev.Rect) {
				issues = append(issues, LayoutIssue{Widget: label.Name, Rule: "label-overlap", Msg: "labels overlap inside " + label.Parent})
			}
		}
	}
	return issues
}

func mustWidget(widgets []Widget, name string) Widget {
	w, ok := findWidget(widgets, name)
	if !ok {
		panic("hud: missing campaign menu widget " + name)
	}
	return w
}
