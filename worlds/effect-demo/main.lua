-- effect-demo: the minimal world proving special-effect models register from
-- world data (#530). data/effects-models/models.toml declares "fx/glow" and
-- "fx/spark"; worldhost registers them before this script runs, so the
-- Game_AddSpecialEffect calls below resolve to live handles instead of failing
-- closed (the gap #530 fixes). Game_Effects() / cmd/litd -autotest then show two
-- effects at the exact coords spawned here — the FSV source of truth.

Game_SetTimeOfDay(12.0)

-- One unit so the world has a non-empty unit set (matches dev-sandbox).
Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), { x = 320, y = 256 }, 90)

-- Two special effects at known coordinates. X+X=Y: spawn at (100,200) and
-- (300,400) -> Game_Effects() must return exactly these two positions.
Game_AddSpecialEffect("fx/glow", { x = 100, y = 200 })
Game_AddSpecialEffect("fx/spark", { x = 300, y = 400 })
