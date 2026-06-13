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
	HUDResourceGold    Key = "hud.resource.gold"
	HUDResourceLumber  Key = "hud.resource.lumber"
	HUDResourceFood    Key = "hud.resource.food"
	HUDVitalLife       Key = "hud.vital.life"
	HUDVitalMana       Key = "hud.vital.mana"
	HUDSelectionPrefix Key = "hud.selection.prefix"
	HUDQueuePrefix     Key = "hud.queue.prefix"
	HUDGroupsPrefix    Key = "hud.groups.prefix"
	HUDMenuOKTrue      Key = "hud.menu.ok_true"
	HUDMenuOKFalse     Key = "hud.menu.ok_false"
	HUDIdleWorker      Key = "hud.widget.idle_worker"
	HUDMinimap         Key = "hud.widget.minimap"
)

var requiredKeys = []string{
	string(HUDResourceGold),
	string(HUDResourceLumber),
	string(HUDResourceFood),
	string(HUDVitalLife),
	string(HUDVitalMana),
	string(HUDSelectionPrefix),
	string(HUDQueuePrefix),
	string(HUDGroupsPrefix),
	string(HUDMenuOKTrue),
	string(HUDMenuOKFalse),
	string(HUDIdleWorker),
	string(HUDMinimap),
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
