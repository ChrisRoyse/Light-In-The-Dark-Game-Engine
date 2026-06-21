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

-- Install the example trigger-authored firebolt spell (#478/#479). The module
-- creates the trigger and binds it to the name the firebolt ability references;
-- a unit that learns + casts firebolt fires it. Required as a sibling chunk
-- (#412) — no engine rebuild, no global injection.
require("scripts/spell_firebolt")
