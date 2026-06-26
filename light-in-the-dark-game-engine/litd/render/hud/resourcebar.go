package hud

import "strconv"

const (
	ResourceGold                       = "gold"
	ResourceLumber                     = "lumber"
	ResourceFood                       = "food"
	ResourceErrorSoundInsufficientGold = "ui.error.insufficient_gold"
	resourceFlashTicks                 = 30
)

type ResourceBarStrings struct {
	Gold   string
	Lumber string
	Food   string
	Upkeep string
}

type ResourceBarState struct {
	Gold     int
	Lumber   int
	FoodUsed int
	FoodCap  int
	Upkeep   int
	Tick     uint32
}

type ResourceBarUpdate struct {
	Dirty         bool   `json:"dirty"`
	Repaints      int    `json:"repaints"`
	FlashVisible  bool   `json:"flashVisible"`
	FlashResource string `json:"flashResource,omitempty"`
}

type ResourceFeedback struct {
	Tick     uint32 `json:"tick"`
	Resource string `json:"resource"`
	Sound    string `json:"sound"`
	Value    int    `json:"value"`
}

type ResourceBar struct {
	Text   *TextBuffer
	Labels ResourceBarStrings

	state       ResourceBarState
	initialized bool

	flashResource string
	flashUntil    uint32
	flashVisible  bool
	feedback      [4]ResourceFeedback
	feedbackCount int
}

func ResourceBarStringsFromHUD(labels HUDStrings) ResourceBarStrings {
	return ResourceBarStrings{
		Gold:   labels.ResourceGold,
		Lumber: labels.ResourceLumber,
		Food:   labels.ResourceFood,
		Upkeep: labels.ResourceUpkeep,
	}
}

func NewResourceBar(text *TextBuffer, labels ResourceBarStrings) ResourceBar {
	return ResourceBar{Text: text, Labels: labels}
}

func (b *ResourceBar) Update(s ResourceBarState) ResourceBarUpdate {
	activeFlash := b.flashActive(s.Tick)
	dirty := !b.initialized ||
		s.Gold != b.state.Gold ||
		s.Lumber != b.state.Lumber ||
		s.FoodUsed != b.state.FoodUsed ||
		s.FoodCap != b.state.FoodCap ||
		s.Upkeep != b.state.Upkeep ||
		activeFlash != b.flashVisible
	if dirty {
		b.setText(s, activeFlash)
	}
	b.state = s
	b.initialized = true
	b.flashVisible = activeFlash
	if !dirty {
		return ResourceBarUpdate{FlashVisible: activeFlash, FlashResource: b.activeFlashResource(activeFlash)}
	}
	return ResourceBarUpdate{Dirty: true, Repaints: 1, FlashVisible: activeFlash, FlashResource: b.activeFlashResource(activeFlash)}
}

func (b *ResourceBar) InsufficientGold(tick uint32, value int) ResourceFeedback {
	return b.rejectResource(tick, ResourceGold, ResourceErrorSoundInsufficientGold, value)
}

func (b *ResourceBar) FeedbackEvents() []ResourceFeedback {
	return b.feedback[:b.feedbackCount]
}

func (b *ResourceBar) flashActive(tick uint32) bool {
	return b.flashResource != "" && tick < b.flashUntil
}

func (b *ResourceBar) activeFlashResource(active bool) string {
	if !active {
		return ""
	}
	return b.flashResource
}

func (b *ResourceBar) rejectResource(tick uint32, resource, sound string, value int) ResourceFeedback {
	b.flashResource = resource
	b.flashUntil = tick + resourceFlashTicks
	event := ResourceFeedback{Tick: tick, Resource: resource, Sound: sound, Value: value}
	if b.feedbackCount < len(b.feedback) {
		b.feedback[b.feedbackCount] = event
		b.feedbackCount++
	} else {
		copy(b.feedback[:], b.feedback[1:])
		b.feedback[len(b.feedback)-1] = event
	}
	return event
}

func (b *ResourceBar) setText(s ResourceBarState, flash bool) {
	if b.Text == nil {
		return
	}
	p := b.Text.reset()
	if flash && b.flashResource == ResourceGold {
		p = append(p, '!')
	}
	p = append(p, b.Labels.Gold...)
	p = append(p, ' ')
	p = strconv.AppendInt(p, int64(s.Gold), 10)
	p = append(p, ' ', ' ')
	if flash && b.flashResource == ResourceLumber {
		p = append(p, '!')
	}
	p = append(p, b.Labels.Lumber...)
	p = append(p, ' ')
	p = strconv.AppendInt(p, int64(s.Lumber), 10)
	p = append(p, ' ', ' ')
	if flash && b.flashResource == ResourceFood {
		p = append(p, '!')
	}
	p = append(p, b.Labels.Food...)
	p = append(p, ' ')
	p = strconv.AppendInt(p, int64(s.FoodUsed), 10)
	p = append(p, '/')
	p = strconv.AppendInt(p, int64(s.FoodCap), 10)
	p = append(p, ' ', ' ')
	p = append(p, b.Labels.Upkeep...)
	p = append(p, ' ')
	p = strconv.AppendInt(p, int64(s.Upkeep), 10)
	b.Text.commit(p)
}
