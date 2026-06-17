-- dev-sandbox: the minimal runtime-loadable world (#268).
--
-- It runs entirely through the sandboxed Lua binding surface against the live
-- game the host bound (Register). No engine rebuild is needed to change it:
-- edit this file and reload.
--
-- Self-contained: the world resolves the types it spawns from its own Lua via
-- Game_UnitType (#393); players come from the bound game via Game_Player. The
-- host only has to DefineUnits the "hfoo" data — no per-world global injection.

-- Set the clock to midday.
Game_SetTimeOfDay(12.0)

-- Spawn one unit at a known location, facing east (90 deg).
Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), { x = 320, y = 256 }, 90)
