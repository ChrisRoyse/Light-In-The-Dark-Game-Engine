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
	luaName string // e.g. "Vec2_Add"
	fnName  string // e.g. "bindVec2Add"
	code    string // the full func declaration
}

// supportedArg returns the Go expression reading a parameter of type typ at Lua
// stack index idx, and whether the type is supported.
func supportedArg(typ string, idx int) (string, bool) {
	switch typ {
	case "Vec2":
		return fmt.Sprintf("argVec2(L, %d)", idx), true
	case "Angle":
		return fmt.Sprintf("argAngle(L, %d)", idx), true
	case "float64":
		return fmt.Sprintf("float64(L.CheckNumber(%d))", idx), true
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
	case "float64":
		return fmt.Sprintf("L.Push(lua.LNumber(%s))", expr), true
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
	recvExpr, ok := supportedArg(recvType, 1)
	if !ok {
		return dispatchBind{}, false
	}
	if len(gs.Returns) > 1 {
		return dispatchBind{}, false
	}

	var b strings.Builder
	fnName := "bind" + strings.ReplaceAll(symbol, ".", "")
	luaName := luaBindingName(symbol)
	fmt.Fprintf(&b, "// %s -> %s%s\n", luaName, symbol, sig)
	fmt.Fprintf(&b, "func %s(L *lua.LState) int {\n", fnName)
	fmt.Fprintf(&b, "\trecv := %s\n", recvExpr)

	args := make([]string, 0, len(gs.Params))
	for i, p := range gs.Params {
		if p.Variadic {
			return dispatchBind{}, false
		}
		expr, ok := supportedArg(p.Type, i+2) // receiver is arg 1
		if !ok {
			return dispatchBind{}, false
		}
		name := fmt.Sprintf("a%d", i)
		fmt.Fprintf(&b, "\t%s := %s\n", name, expr)
		args = append(args, name)
	}
	call := fmt.Sprintf("recv.%s(%s)", method, strings.Join(args, ", "))

	if len(gs.Returns) == 0 {
		fmt.Fprintf(&b, "\t%s\n\treturn 0\n}\n", call)
	} else {
		push, ok := supportedRet(gs.Returns[0], "res")
		if !ok {
			return dispatchBind{}, false
		}
		fmt.Fprintf(&b, "\tres := %s\n\t%s\n\treturn 1\n}\n", call, push)
	}
	return dispatchBind{luaName: luaName, fnName: fnName, code: b.String()}, true
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
	b.WriteString("// registerValueMath installs the generated value-math globals on L.\n")
	b.WriteString("func registerValueMath(L *lua.LState) {\n")
	for _, d := range binds {
		fmt.Fprintf(&b, "\tL.SetGlobal(%q, L.NewFunction(%s))\n", d.luaName, d.fnName)
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
