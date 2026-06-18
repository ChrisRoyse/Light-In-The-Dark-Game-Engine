package main

// emit_luabind_dispatch.go generates EXECUTABLE Lua binding dispatch from the
// manifest (#267) — the layer the descriptor list (emit_lua.go) only described.
// For each mapped symbol whose signature is fully marshalable, it emits a
// gopher-lua `func(*lua.LState) int` following the register.go ABI (read args
// 1-based through the stable argVec2/argAngle readers + L.CheckNumber, call the
// api verb, push the marshaled result) plus a registerValueMath installer.
//
// Coverage is deliberately bounded and fail-closed: a symbol whose receiver,
// params, or return types are not in the supported set (value types Vec2/Angle/
// float64 today), or that is variadic/generic/multi-return, is NOT emitted —
// and the generated header reports how many were bound vs skipped, so the gap
// is visible, never a silent drop. Broadening the type map (handles, enums,
// slices, Game-bound verbs) extends supportedArg/supportedRet without changing
// the ABI.

import (
	"fmt"
	"go/format"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// luabindDispatchFile is the generated dispatch output path.
const luabindDispatchFile = "bindings_dispatch_gen.go"

// WriteLuaDispatch writes the generated dispatch source.
func WriteLuaDispatch(m Manifest) error {
	if err := os.MkdirAll(luabindDir, 0o755); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(luabindDir, luabindDispatchFile), []byte(RenderLuaDispatch(m)), 0o644)
}

// dispatchBind is one emitted binding.
type dispatchBind struct {
	luaName   string // e.g. "Vec2_Add"
	fnName    string // e.g. "bindVec2Add"
	code      string // the full func/method declaration
	gameBound bool   // true => a method on gameBinder (needs the *Game)
}

// supportedArg returns the Go expression reading a parameter of type typ at Lua
// stack index idx, and whether the type is supported.
func supportedArg(typ string, idx int) (string, bool) {
	switch typ {
	case "Vec2":
		return fmt.Sprintf("argVec2(L, %d)", idx), true
	case "Angle":
		return fmt.Sprintf("argAngle(L, %d)", idx), true
	case "Rect":
		return fmt.Sprintf("argRect(L, %d)", idx), true
	case "Unit":
		return fmt.Sprintf("argUnit(L, %d)", idx), true
	case "Item":
		return fmt.Sprintf("argItem(L, %d)", idx), true
	case "Destructable":
		return fmt.Sprintf("argDestructable(L, %d)", idx), true
	case "Missile":
		return fmt.Sprintf("argMissile(L, %d)", idx), true
	case "Effect":
		return fmt.Sprintf("argEffect(L, %d)", idx), true
	case "Player":
		return fmt.Sprintf("argPlayer(L, %d)", idx), true
	case "Timer":
		return fmt.Sprintf("argTimer(L, %d)", idx), true
	case "UnitType":
		return fmt.Sprintf("argUnitType(L, %d)", idx), true
	case "ItemType":
		return fmt.Sprintf("argItemType(L, %d)", idx), true
	case "BuffType":
		return fmt.Sprintf("argBuffType(L, %d)", idx), true
	case "Order":
		return fmt.Sprintf("argOrder(L, %d)", idx), true
	case "Event":
		return fmt.Sprintf("argEvent(L, %d)", idx), true
	case "Region":
		return fmt.Sprintf("argRegion(L, %d)", idx), true
	case "Subscription":
		return fmt.Sprintf("argSubscription(L, %d)", idx), true
	case "EventKind":
		return fmt.Sprintf("argEventKind(L, %d)", idx), true
	case "float64":
		return fmt.Sprintf("float64(L.CheckNumber(%d))", idx), true
	case "int":
		return fmt.Sprintf("L.CheckInt(%d)", idx), true
	case "int32":
		return fmt.Sprintf("int32(L.CheckInt(%d))", idx), true
	case "int64":
		return fmt.Sprintf("L.CheckInt64(%d)", idx), true
	case "uint32":
		return fmt.Sprintf("uint32(L.CheckInt(%d))", idx), true
	case "uint8":
		return fmt.Sprintf("uint8(L.CheckInt(%d))", idx), true
	case "string":
		return fmt.Sprintf("L.CheckString(%d)", idx), true
	case "bool":
		return fmt.Sprintf("L.CheckBool(%d)", idx), true
	case "time.Duration":
		return fmt.Sprintf("argDuration(L, %d)", idx), true
	case "Race":
		return fmt.Sprintf("argRace(L, %d)", idx), true
	case "Difficulty":
		return fmt.Sprintf("argDifficulty(L, %d)", idx), true
	case "FogState":
		return fmt.Sprintf("argFogState(L, %d)", idx), true
	case "Controller":
		return fmt.Sprintf("argController(L, %d)", idx), true
	case "AllianceFlags":
		return fmt.Sprintf("argAllianceFlags(L, %d)", idx), true
	case "AbilityField":
		return fmt.Sprintf("argAbilityField(L, %d)", idx), true
	case "CameraField":
		return fmt.Sprintf("argCameraField(L, %d)", idx), true
	case "AbilityRef":
		return fmt.Sprintf("argAbilityRef(L, %d)", idx), true
	case "GameSpeed":
		return fmt.Sprintf("argGameSpeed(L, %d)", idx), true
	case "MapFlag":
		return fmt.Sprintf("argMapFlag(L, %d)", idx), true
	case "UnitClass":
		return fmt.Sprintf("argUnitClass(L, %d)", idx), true
	case "SoundChannel":
		return fmt.Sprintf("argSoundChannel(L, %d)", idx), true
	default:
		return "", false
	}
}

// supportedRet returns the Go statement pushing a result of type typ (held in
// the variable named expr), and whether the type is supported.
func supportedRet(typ, expr string) (string, bool) {
	switch typ {
	case "Vec2":
		return fmt.Sprintf("L.Push(vec2ToLua(L, %s))", expr), true
	case "Angle":
		return fmt.Sprintf("L.Push(angleToLua(%s))", expr), true
	case "Rect":
		return fmt.Sprintf("L.Push(rectToLua(L, %s))", expr), true
	case "Unit", "Item", "Destructable", "Missile", "Effect", "Player", "Timer",
		"UnitType", "ItemType", "BuffType", "Order", "Event", "Region", "Subscription":
		return fmt.Sprintf("L.Push(handleToLua(L, %s))", expr), true
	case "float64", "int", "int32", "int64", "uint32", "uint8",
		"Race", "Difficulty", "FogState", "Controller", "AllianceFlags",
		"AbilityField", "CameraField", "AbilityRef", "EventKind":
		// Plain + integer-enum returns push as a number (LNumber converts any
		// numeric value; the enum type name is never referenced here).
		return fmt.Sprintf("L.Push(lua.LNumber(%s))", expr), true
	case "string":
		return fmt.Sprintf("L.Push(lua.LString(%s))", expr), true
	case "bool":
		return fmt.Sprintf("L.Push(lua.LBool(%s))", expr), true
	case "[]Unit", "[]Item", "[]Destructable", "[]Missile", "[]Effect", "[]Player":
		return fmt.Sprintf("L.Push(handleSliceToLua(L, %s))", expr), true
	case "[]int", "[]int32", "[]int64", "[]uint32", "[]uint8":
		return fmt.Sprintf("L.Push(intSliceToLua(L, %s))", expr), true
	case "[]string":
		return fmt.Sprintf("L.Push(stringSliceToLua(L, %s))", expr), true
	case "time.Duration":
		// Lua surface is seconds (a method call, no api/time import in the gen file).
		return fmt.Sprintf("L.Push(lua.LNumber(%s.Seconds()))", expr), true
	default:
		return "", false
	}
}

// emitDispatch returns the binding code for symbol with goSignature sig, and
// whether it is supported. Only value-receiver methods (Vec2/Angle) with
// supported params and zero-or-one supported return are emitted today.
func emitDispatch(symbol, sig string) (dispatchBind, bool) {
	gs, err := parseGoSignature(sig)
	if err != nil || len(gs.TypeParams) > 0 {
		return dispatchBind{}, false
	}
	dot := strings.IndexByte(symbol, '.')
	if dot < 0 {
		return dispatchBind{}, false // free functions need a *Game / other types — later
	}
	recvType, method := symbol[:dot], symbol[dot+1:]
	if len(gs.Returns) > 1 {
		return dispatchBind{}, false
	}

	// A Game-receiver verb becomes a method on gameBinder using the bound b.g;
	// its params start at Lua arg 1 (no receiver arg). Any other receiver is a
	// value/handle the script passes as arg 1.
	gameBound := recvType == "Game"
	argStart := 2
	var recvLine, callRecv string
	if gameBound {
		argStart = 1
		callRecv = "b.g"
	} else {
		recvExpr, ok := supportedArg(recvType, 1)
		if !ok {
			return dispatchBind{}, false
		}
		recvLine = fmt.Sprintf("\trecv := %s\n", recvExpr)
		callRecv = "recv"
	}

	var body strings.Builder
	args := make([]string, 0, len(gs.Params))
	for i, p := range gs.Params {
		if p.Variadic {
			return dispatchBind{}, false
		}
		expr, ok := supportedArg(p.Type, i+argStart)
		if !ok {
			return dispatchBind{}, false
		}
		name := fmt.Sprintf("a%d", i)
		fmt.Fprintf(&body, "\t%s := %s\n", name, expr)
		args = append(args, name)
	}
	call := fmt.Sprintf("%s.%s(%s)", callRecv, method, strings.Join(args, ", "))
	var tail string
	if len(gs.Returns) == 0 {
		tail = fmt.Sprintf("\t%s\n\treturn 0\n", call)
	} else {
		push, ok := supportedRet(gs.Returns[0], "res")
		if !ok {
			return dispatchBind{}, false
		}
		tail = fmt.Sprintf("\tres := %s\n\t%s\n\treturn 1\n", call, push)
	}

	fnName := "bind" + strings.ReplaceAll(symbol, ".", "")
	luaName := luaBindingName(symbol)
	var b strings.Builder
	fmt.Fprintf(&b, "// %s -> %s%s\n", luaName, symbol, sig)
	if gameBound {
		fmt.Fprintf(&b, "func (b gameBinder) %s(L *lua.LState) int {\n", fnName)
	} else {
		fmt.Fprintf(&b, "func %s(L *lua.LState) int {\n", fnName)
		b.WriteString(recvLine)
	}
	b.WriteString(body.String())
	b.WriteString(tail)
	b.WriteString("}\n")
	return dispatchBind{luaName: luaName, fnName: fnName, code: b.String(), gameBound: gameBound}, true
}

// collectDispatch returns the emitted bindings (sorted) plus the total mapped
// count, for the header's coverage report.
func collectDispatch(m Manifest) (binds []dispatchBind, total int) {
	seen := map[string]bool{}
	for _, f := range m.Functions {
		if f.Disposition != "mapped" || f.GoMapping == nil {
			continue
		}
		if seen[f.GoMapping.Symbol] {
			continue
		}
		seen[f.GoMapping.Symbol] = true
		total++
		if d, ok := emitDispatch(f.GoMapping.Symbol, f.GoMapping.GoSignature); ok {
			binds = append(binds, d)
		}
	}
	sort.Slice(binds, func(i, j int) bool { return binds[i].luaName < binds[j].luaName })
	return binds, total
}

// RenderLuaDispatch renders the generated dispatch source (byte-deterministic).
func RenderLuaDispatch(m Manifest) string {
	binds, total := collectDispatch(m)
	var b strings.Builder
	b.WriteString("// Code generated by jassgen; DO NOT EDIT.\n")
	fmt.Fprintf(&b, "// Lua binding dispatch derived from api-manifest.json (#267): %d of %d mapped\n", len(binds), total)
	b.WriteString("// verbs bound. The rest need handle/enum/slice/Game/variadic marshaling not yet\n")
	b.WriteString("// emitted by the generator and are intentionally unbound (no silent drop).\n\n")
	b.WriteString("package luabind\n\n")
	b.WriteString("import (\n\tlua \"github.com/yuin/gopher-lua\"\n)\n\n")
	b.WriteString("// registerGenerated installs the generated value/handle-verb globals on L.\n")
	b.WriteString("// These verbs need no game: value types are pure and a handle/Player userdata\n")
	b.WriteString("// self-carries its *Game. Game-receiver verbs install via registerGameBound.\n")
	b.WriteString("func registerGenerated(L *lua.LState) {\n")
	for _, d := range binds {
		if d.gameBound {
			continue
		}
		fmt.Fprintf(&b, "\tL.SetGlobal(%q, L.NewFunction(%s))\n", d.luaName, d.fnName)
	}
	b.WriteString("}\n\n")
	b.WriteString("// registerGameBound installs the generated Game-receiver verb globals on L,\n")
	b.WriteString("// each bound through b (the implicit game receiver; the script passes no game\n")
	b.WriteString("// arg). gameBinder is defined in register.go so this file imports only lua.\n")
	b.WriteString("func registerGameBound(L *lua.LState, b gameBinder) {\n")
	for _, d := range binds {
		if !d.gameBound {
			continue
		}
		fmt.Fprintf(&b, "\tL.SetGlobal(%q, L.NewFunction(b.%s))\n", d.luaName, d.fnName)
	}
	b.WriteString("}\n\n")
	for _, d := range binds {
		b.WriteString(d.code)
		b.WriteString("\n")
	}
	// gofmt the generated source so the committed file is canonical and the
	// -check gate stays byte-stable. A format error means the generator emitted
	// invalid Go — surface it loudly rather than committing broken output.
	out, err := format.Source([]byte(b.String()))
	if err != nil {
		panic(fmt.Sprintf("jassgen: generated lua dispatch is not valid Go: %v", err))
	}
	return string(out)
}
