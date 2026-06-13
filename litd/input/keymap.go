package input

import (
	"bytes"
	"fmt"
	"io"
	"io/fs"
	"os"
	"sort"
	"strconv"
	"strings"

	"github.com/BurntSushi/toml"
)

const (
	KeymapCommandCardSlots = 12
	DefaultKeymapPath      = "input/keymap-grid.toml"
	ClassicKeymapPath      = "input/keymap-classic.toml"
)

type KeyContext string

const (
	ContextGame      KeyContext = "game"
	ContextTargeting KeyContext = "targeting"
	ContextReplay    KeyContext = "replay"
)

type KeyChord struct {
	Key   string `json:"key"`
	Ctrl  bool   `json:"ctrl,omitempty"`
	Shift bool   `json:"shift,omitempty"`
	Alt   bool   `json:"alt,omitempty"`
}

type KeyBinding struct {
	Context KeyContext `json:"context"`
	Action  string     `json:"action"`
	Chord   KeyChord   `json:"chord"`
}

type Keymap struct {
	Profile  string       `json:"profile"`
	Bindings []KeyBinding `json:"bindings"`
}

type rawKeymap struct {
	Profile   string              `toml:"profile"`
	Game      map[string][]string `toml:"game"`
	Targeting map[string][]string `toml:"targeting"`
	Replay    map[string][]string `toml:"replay"`
}

func CommandCardSlotAction(slot int) string {
	return "card.slot." + strconv.Itoa(slot)
}

func CommandCardSlot(action string) (uint8, bool) {
	const prefix = "card.slot."
	if !strings.HasPrefix(action, prefix) {
		return 0, false
	}
	n, err := strconv.Atoi(strings.TrimPrefix(action, prefix))
	if err != nil || n < 0 || n >= KeymapCommandCardSlots {
		return 0, false
	}
	return uint8(n), true
}

func LoadKeymap(fsys fs.FS, file string) (*Keymap, error) {
	blob, err := fs.ReadFile(fsys, file)
	if err != nil {
		return nil, fmt.Errorf("keymap: read %s: %w", file, err)
	}
	return ReadKeymap(file, blob)
}

func LoadKeymapFile(file string) (*Keymap, error) {
	blob, err := os.ReadFile(file)
	if err != nil {
		return nil, fmt.Errorf("keymap: read %s: %w", file, err)
	}
	return ReadKeymap(file, blob)
}

func ReadKeymap(file string, blob []byte) (*Keymap, error) {
	var raw rawKeymap
	md, err := toml.Decode(string(blob), &raw)
	if err != nil {
		return nil, fmt.Errorf("keymap: %s: %w", file, err)
	}
	for _, un := range md.Undecoded() {
		return nil, fmt.Errorf("keymap: %s: unknown field %q", file, un.String())
	}
	return compileKeymap(file, raw)
}

func DefaultGridKeymap() *Keymap {
	m, err := compileKeymap("<default-grid>", defaultGridRaw())
	if err != nil {
		panic(err)
	}
	return m
}

func DefaultClassicKeymap() *Keymap {
	m, err := compileKeymap("<default-classic>", defaultClassicRaw())
	if err != nil {
		panic(err)
	}
	return m
}

func (m *Keymap) Resolve(ctx KeyContext, chord KeyChord) (KeyBinding, bool) {
	chord = chord.Normalize()
	for _, b := range m.Bindings {
		if b.Context == ctx && b.Chord == chord {
			return b, true
		}
	}
	return KeyBinding{}, false
}

func (m *Keymap) Binding(ctx KeyContext, action string) (KeyBinding, bool) {
	for _, b := range m.Bindings {
		if b.Context == ctx && b.Action == action {
			return b, true
		}
	}
	return KeyBinding{}, false
}

func (m *Keymap) CommandCardHotkeys(ctx KeyContext) [KeymapCommandCardSlots]string {
	var out [KeymapCommandCardSlots]string
	for i := 0; i < KeymapCommandCardSlots; i++ {
		if b, ok := m.Binding(ctx, CommandCardSlotAction(i)); ok {
			out[i] = b.Chord.String()
		}
	}
	return out
}

func (m *Keymap) Overlay(override *Keymap) (*Keymap, error) {
	if m == nil {
		return nil, fmt.Errorf("keymap: nil base keymap")
	}
	if override == nil {
		cp := *m
		cp.Bindings = append([]KeyBinding(nil), m.Bindings...)
		return &cp, nil
	}
	out := &Keymap{Profile: m.Profile, Bindings: append([]KeyBinding(nil), m.Bindings...)}
	if override.Profile != "" {
		out.Profile = override.Profile
	}
	for _, ob := range override.Bindings {
		replaced := false
		for i := range out.Bindings {
			if out.Bindings[i].Context == ob.Context && out.Bindings[i].Action == ob.Action {
				out.Bindings[i] = ob
				replaced = true
				break
			}
		}
		if !replaced {
			out.Bindings = append(out.Bindings, ob)
		}
	}
	if err := validateKeyBindings("<overlay>", out.Bindings); err != nil {
		return nil, err
	}
	sort.Slice(out.Bindings, func(i, j int) bool {
		if out.Bindings[i].Context != out.Bindings[j].Context {
			return out.Bindings[i].Context < out.Bindings[j].Context
		}
		return out.Bindings[i].Action < out.Bindings[j].Action
	})
	return out, nil
}

func (m *Keymap) WriteTOML(w io.Writer) error {
	if m == nil {
		return fmt.Errorf("keymap: nil keymap")
	}
	var b bytes.Buffer
	if m.Profile != "" {
		fmt.Fprintf(&b, "profile = %q\n", m.Profile)
	}
	for _, ctx := range []KeyContext{ContextGame, ContextTargeting, ContextReplay} {
		bindings := m.bindingsFor(ctx)
		if len(bindings) == 0 {
			continue
		}
		if b.Len() > 0 {
			b.WriteByte('\n')
		}
		fmt.Fprintf(&b, "[%s]\n", ctx)
		for _, binding := range bindings {
			fmt.Fprintf(&b, "%s = %s\n", tomlKey(binding.Action), binding.Chord.tomlArray())
		}
	}
	_, err := w.Write(b.Bytes())
	return err
}

func (m *Keymap) bindingsFor(ctx KeyContext) []KeyBinding {
	out := make([]KeyBinding, 0, len(m.Bindings))
	for _, b := range m.Bindings {
		if b.Context == ctx {
			out = append(out, b)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Action < out[j].Action })
	return out
}

func ParseKeyChord(parts []string) (KeyChord, error) {
	var chord KeyChord
	if len(parts) == 1 {
		token := strings.TrimSpace(parts[0])
		if token == "" {
			return KeyChord{}, fmt.Errorf("empty key token")
		}
		return KeyChord{Key: canonicalKey(token)}, nil
	}
	for _, part := range parts {
		token := strings.TrimSpace(part)
		if token == "" {
			return KeyChord{}, fmt.Errorf("empty key token")
		}
		switch strings.ToLower(token) {
		case "ctrl", "control":
			chord.Ctrl = true
		case "shift":
			chord.Shift = true
		case "alt", "option":
			chord.Alt = true
		default:
			if chord.Key != "" {
				return KeyChord{}, fmt.Errorf("multiple keys %q and %q", chord.Key, token)
			}
			chord.Key = canonicalKey(token)
		}
	}
	if chord.Key == "" {
		return KeyChord{}, fmt.Errorf("missing key")
	}
	return chord, nil
}

func tomlKey(key string) string {
	for _, r := range key {
		if (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9') || r == '_' || r == '-' {
			continue
		}
		return strconv.Quote(key)
	}
	return key
}

func Key(key string, modifiers ...string) KeyChord {
	parts := make([]string, 0, len(modifiers)+1)
	parts = append(parts, key)
	parts = append(parts, modifiers...)
	chord, err := ParseKeyChord(parts)
	if err != nil {
		return KeyChord{}
	}
	return chord
}

func (c KeyChord) Normalize() KeyChord {
	c.Key = canonicalKey(c.Key)
	return c
}

func (c KeyChord) String() string {
	c = c.Normalize()
	var parts []string
	if c.Ctrl {
		parts = append(parts, "Ctrl")
	}
	if c.Shift {
		parts = append(parts, "Shift")
	}
	if c.Alt {
		parts = append(parts, "Alt")
	}
	parts = append(parts, c.Key)
	return strings.Join(parts, "+")
}

func (c KeyChord) tomlArray() string {
	c = c.Normalize()
	parts := []string{c.Key}
	if c.Ctrl {
		parts = append(parts, "Ctrl")
	}
	if c.Shift {
		parts = append(parts, "Shift")
	}
	if c.Alt {
		parts = append(parts, "Alt")
	}
	for i := range parts {
		parts[i] = strconv.Quote(parts[i])
	}
	return "[" + strings.Join(parts, ", ") + "]"
}

func compileKeymap(file string, raw rawKeymap) (*Keymap, error) {
	profile := strings.TrimSpace(raw.Profile)
	if profile == "" {
		profile = "grid"
	}
	out := &Keymap{Profile: profile}
	for _, ctx := range []struct {
		name     KeyContext
		bindings map[string][]string
	}{
		{name: ContextGame, bindings: raw.Game},
		{name: ContextTargeting, bindings: raw.Targeting},
		{name: ContextReplay, bindings: raw.Replay},
	} {
		if err := appendContextBindings(file, out, ctx.name, ctx.bindings); err != nil {
			return nil, err
		}
	}
	sort.Slice(out.Bindings, func(i, j int) bool {
		if out.Bindings[i].Context != out.Bindings[j].Context {
			return out.Bindings[i].Context < out.Bindings[j].Context
		}
		return out.Bindings[i].Action < out.Bindings[j].Action
	})
	return out, nil
}

func appendContextBindings(file string, out *Keymap, ctx KeyContext, bindings map[string][]string) error {
	actions := make([]string, 0, len(bindings))
	for action := range bindings {
		actions = append(actions, action)
	}
	sort.Strings(actions)
	seen := map[string]string{}
	for _, action := range actions {
		chord, err := ParseKeyChord(bindings[action])
		if err != nil {
			return fmt.Errorf("keymap: %s: %s.%s: %w", file, ctx, action, err)
		}
		key := chord.String()
		if other, ok := seen[key]; ok {
			return fmt.Errorf("keymap: %s: context %q key %q conflicts: %s and %s", file, ctx, key, other, action)
		}
		seen[key] = action
		out.Bindings = append(out.Bindings, KeyBinding{Context: ctx, Action: action, Chord: chord})
	}
	return nil
}

func validateKeyBindings(file string, bindings []KeyBinding) error {
	seen := map[KeyContext]map[string]string{}
	for _, b := range bindings {
		if seen[b.Context] == nil {
			seen[b.Context] = map[string]string{}
		}
		key := b.Chord.String()
		if other, ok := seen[b.Context][key]; ok {
			return fmt.Errorf("keymap: %s: context %q key %q conflicts: %s and %s", file, b.Context, key, other, b.Action)
		}
		seen[b.Context][key] = b.Action
	}
	return nil
}

func canonicalKey(key string) string {
	key = strings.TrimSpace(key)
	if len(key) == 1 {
		return strings.ToUpper(key)
	}
	switch strings.ToLower(key) {
	case "esc", "escape":
		return "Esc"
	case "space":
		return "Space"
	case "tab":
		return "Tab"
	case "shift":
		return "Shift"
	case "ctrl", "control":
		return "Ctrl"
	case "alt", "option":
		return "Alt"
	default:
		if strings.HasPrefix(strings.ToLower(key), "f") {
			return strings.ToUpper(key)
		}
		return key
	}
}

func defaultGridRaw() rawKeymap {
	return rawKeymap{
		Profile: "grid",
		Game: map[string][]string{
			CommandCardSlotAction(0):  {"Q"},
			CommandCardSlotAction(1):  {"W"},
			CommandCardSlotAction(2):  {"E"},
			CommandCardSlotAction(3):  {"R"},
			CommandCardSlotAction(4):  {"A"},
			CommandCardSlotAction(5):  {"S"},
			CommandCardSlotAction(6):  {"D"},
			CommandCardSlotAction(7):  {"F"},
			CommandCardSlotAction(8):  {"Z"},
			CommandCardSlotAction(9):  {"X"},
			CommandCardSlotAction(10): {"C"},
			CommandCardSlotAction(11): {"V"},
			"cancel":                  {"Esc"},
			"group.0":                 {"0"},
			"group.1":                 {"1"},
			"group.2":                 {"2"},
			"group.3":                 {"3"},
			"group.4":                 {"4"},
			"group.5":                 {"5"},
			"group.6":                 {"6"},
			"group.7":                 {"7"},
			"group.8":                 {"8"},
			"group.9":                 {"9"},
			"group.add.0":             {"0", "Shift"},
			"group.add.1":             {"1", "Shift"},
			"group.add.2":             {"2", "Shift"},
			"group.add.3":             {"3", "Shift"},
			"group.add.4":             {"4", "Shift"},
			"group.add.5":             {"5", "Shift"},
			"group.add.6":             {"6", "Shift"},
			"group.add.7":             {"7", "Shift"},
			"group.add.8":             {"8", "Shift"},
			"group.add.9":             {"9", "Shift"},
			"group.assign.0":          {"0", "Ctrl"},
			"group.assign.1":          {"1", "Ctrl"},
			"group.assign.2":          {"2", "Ctrl"},
			"group.assign.3":          {"3", "Ctrl"},
			"group.assign.4":          {"4", "Ctrl"},
			"group.assign.5":          {"5", "Ctrl"},
			"group.assign.6":          {"6", "Ctrl"},
			"group.assign.7":          {"7", "Ctrl"},
			"group.assign.8":          {"8", "Ctrl"},
			"group.assign.9":          {"9", "Ctrl"},
			"menu":                    {"F10"},
			"queue":                   {"Shift"},
			"subgroup.next":           {"Tab"},
		},
		Targeting: map[string][]string{
			"cancel":  {"Esc"},
			"confirm": {"MouseLeft"},
			"queue":   {"Shift"},
		},
		Replay: map[string][]string{
			"cancel": {"Esc"},
			"menu":   {"F10"},
			"pause":  {"Space"},
		},
	}
}

func defaultClassicRaw() rawKeymap {
	raw := defaultGridRaw()
	raw.Profile = "classic"
	raw.Game[CommandCardSlotAction(0)] = []string{"M"}
	raw.Game[CommandCardSlotAction(1)] = []string{"S"}
	raw.Game[CommandCardSlotAction(2)] = []string{"H"}
	raw.Game[CommandCardSlotAction(4)] = []string{"A"}
	raw.Game[CommandCardSlotAction(5)] = []string{"P"}
	raw.Game[CommandCardSlotAction(6)] = []string{"L"}
	raw.Game[CommandCardSlotAction(8)] = []string{"D"}
	return raw
}
