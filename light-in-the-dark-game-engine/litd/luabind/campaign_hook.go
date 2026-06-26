package luabind

import (
	"bytes"
	"fmt"
	"io/fs"
	"strings"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
	lua "github.com/yuin/gopher-lua"
)

const (
	DefaultCampaignHookInstructionBudget int64 = 500_000
	DefaultCampaignHookMemoryBudget      int64 = 4 << 20
)

type CampaignHookOptions struct {
	InstructionBudget int64
	MemoryBudget      int64
}

func RunCampaignHook(store *api.Storage, def campaign.Definition, scripts fs.FS, label, missionID string, outcome campaign.HookOutcome, opts CampaignHookOptions) (campaign.Transition, error) {
	if store == nil {
		return campaign.Transition{}, fmt.Errorf("luabind: campaign hook: nil storage")
	}
	if scripts == nil {
		return campaign.Transition{}, fmt.Errorf("luabind: campaign hook: nil script filesystem")
	}
	hook := hookName(def.Hooks, outcome)
	if hook == "" {
		return campaign.Transition{}, fmt.Errorf("luabind: campaign hook: no %s hook configured for campaign %s", outcome, def.ID)
	}
	if opts.InstructionBudget == 0 {
		opts.InstructionBudget = DefaultCampaignHookInstructionBudget
	}
	if opts.MemoryBudget == 0 {
		opts.MemoryBudget = DefaultCampaignHookMemoryBudget
	}

	scratch, err := scratchGameWithStorage(store)
	if err != nil {
		return campaign.Transition{}, err
	}
	interp := NewSandbox(SandboxOptions{
		InstructionBudget: opts.InstructionBudget,
		MemoryBudget:      opts.MemoryBudget,
	})
	defer interp.Close()
	if err := Register(interp.L, scratch); err != nil {
		return campaign.Transition{}, fmt.Errorf("luabind: campaign hook: register: %w", err)
	}
	reg := NewChunkRegistry()
	defer reg.Close()
	if _, err := LoadWorldFS(interp.L, reg, scripts, label); err != nil {
		return campaign.Transition{}, err
	}
	if opts.InstructionBudget > 0 {
		interp.L.SetInstructionBudget(opts.InstructionBudget)
	}
	result, err := callCampaignHook(interp.L, def, missionID, outcome, hook)
	if err != nil {
		return campaign.Transition{}, err
	}
	return campaign.CommitHookResult(store, scratch.Storage(), def, missionID, outcome, result)
}

func scratchGameWithStorage(store *api.Storage) (*api.Game, error) {
	var buf bytes.Buffer
	if err := store.Save(&buf); err != nil {
		return nil, fmt.Errorf("luabind: campaign hook: clone storage: %w", err)
	}
	g, err := api.NewGame(api.GameOptions{})
	if err != nil {
		return nil, fmt.Errorf("luabind: campaign hook: scratch game: %w", err)
	}
	if err := g.Storage().Load(bytes.NewReader(buf.Bytes())); err != nil {
		return nil, fmt.Errorf("luabind: campaign hook: load scratch storage: %w", err)
	}
	return g, nil
}

func hookName(h campaign.Hooks, outcome campaign.HookOutcome) string {
	switch outcome {
	case campaign.OutcomeStart:
		return h.OnStart
	case campaign.OutcomeComplete:
		return h.OnComplete
	case campaign.OutcomeFail:
		return h.OnFail
	default:
		return ""
	}
}

func callCampaignHook(L *lua.LState, def campaign.Definition, missionID string, outcome campaign.HookOutcome, hook string) (campaign.HookResult, error) {
	fn := L.GetGlobal(hook)
	if fn.Type() != lua.LTFunction {
		return campaign.HookResult{}, fmt.Errorf("luabind: campaign hook %s is %s, want function", hook, fn.Type())
	}
	ctx := L.NewTable()
	ctx.RawSetString("campaign", lua.LString(def.ID))
	ctx.RawSetString("mission", lua.LString(missionID))
	ctx.RawSetString("outcome", lua.LString(string(outcome)))
	ctx.RawSetString("category", lua.LString(campaign.StorageCategory(def.ID)))
	ctx.RawSetString("cachePrefix", lua.LString("cache:"))

	L.Push(fn)
	L.Push(ctx)
	if err := L.PCall(1, 1, nil); err != nil {
		return campaign.HookResult{}, fmt.Errorf("luabind: campaign hook %s: %w", hook, err)
	}
	ret := L.Get(-1)
	L.Pop(1)
	t, ok := ret.(*lua.LTable)
	if !ok {
		return campaign.HookResult{}, fmt.Errorf("luabind: campaign hook %s returned %s, want table", hook, ret.Type())
	}
	return luaHookResult(t)
}

func luaHookResult(t *lua.LTable) (campaign.HookResult, error) {
	var out campaign.HookResult
	if v := t.RawGetString("next"); v != lua.LNil {
		s, ok := v.(lua.LString)
		if !ok {
			return campaign.HookResult{}, fmt.Errorf("luabind: campaign hook result next is %s, want string", v.Type())
		}
		out.NextMissionID = string(s)
	}
	heroes, err := luaHeroes(t.RawGetString("heroes"))
	if err != nil {
		return campaign.HookResult{}, err
	}
	out.Heroes = heroes
	cache, err := luaStringArray(t.RawGetString("cache"), "cache")
	if err != nil {
		return campaign.HookResult{}, err
	}
	out.CacheKeys = cache
	log, err := luaStringArray(t.RawGetString("log"), "log")
	if err != nil {
		return campaign.HookResult{}, err
	}
	out.Log = log
	return out, nil
}

func luaHeroes(v lua.LValue) ([]campaign.HeroCarryOver, error) {
	if v == lua.LNil {
		return nil, nil
	}
	tab, ok := v.(*lua.LTable)
	if !ok {
		return nil, fmt.Errorf("luabind: campaign hook result heroes is %s, want array table", v.Type())
	}
	var out []campaign.HeroCarryOver
	for i := 1; ; i++ {
		raw := tab.RawGetInt(i)
		if raw == lua.LNil {
			break
		}
		h, ok := raw.(*lua.LTable)
		if !ok {
			return nil, fmt.Errorf("luabind: campaign hook heroes[%d] is %s, want table", i, raw.Type())
		}
		name, err := luaStringField(h, "name", fmt.Sprintf("heroes[%d].name", i))
		if err != nil {
			return nil, err
		}
		levelv := h.RawGetString("level")
		level, ok := levelv.(lua.LNumber)
		if !ok {
			return nil, fmt.Errorf("luabind: campaign hook heroes[%d].level is %s, want number", i, levelv.Type())
		}
		items, err := luaStringArray(h.RawGetString("items"), fmt.Sprintf("heroes[%d].items", i))
		if err != nil {
			return nil, err
		}
		out = append(out, campaign.HeroCarryOver{Name: name, Level: int(level), Items: items})
	}
	return out, nil
}

func luaStringField(t *lua.LTable, key, label string) (string, error) {
	v := t.RawGetString(key)
	s, ok := v.(lua.LString)
	if !ok {
		return "", fmt.Errorf("luabind: campaign hook %s is %s, want string", label, v.Type())
	}
	return strings.TrimSpace(string(s)), nil
}

func luaStringArray(v lua.LValue, label string) ([]string, error) {
	if v == lua.LNil {
		return nil, nil
	}
	tab, ok := v.(*lua.LTable)
	if !ok {
		return nil, fmt.Errorf("luabind: campaign hook result %s is %s, want array table", label, v.Type())
	}
	var out []string
	for i := 1; ; i++ {
		raw := tab.RawGetInt(i)
		if raw == lua.LNil {
			break
		}
		s, ok := raw.(lua.LString)
		if !ok {
			return nil, fmt.Errorf("luabind: campaign hook result %s[%d] is %s, want string", label, i, raw.Type())
		}
		out = append(out, strings.TrimSpace(string(s)))
	}
	return out, nil
}
