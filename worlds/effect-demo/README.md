# effect-demo

Minimal runtime-loadable world that exercises the special-effect model registry
(#530). `data/effects-models/models.toml` declares two effect models; `worldhost`
assigns each a deterministic sim ModelID at load and registers it via
`RegisterEffectModel`, so `main.lua`'s `Game_AddSpecialEffect("fx/glow", ...)` and
`("fx/spark", ...)` resolve to live handles.

FSV:

```bash
go run ./cmd/litd -world worlds/effect-demo -autotest -ticks 0
# state JSON "effects" array shows two entries at (100,200) and (300,400)
```

Before #530 those `AddSpecialEffect` calls failed closed (`unknown model`) because
nothing registered effect models from world data.
