-- dev-sandbox: the minimal runtime-loadable world (#268).
--
-- It runs entirely through the sandboxed Lua binding surface against the live
-- game the host bound (Register). No engine rebuild is needed to change it:
-- edit this file and reload.
--
-- `footman` is a world data-table binding the host injects before load (the
-- UnitType the world spawns). Players come from the bound game via Game_Player.

-- Set the clock to midday.
Game_SetTimeOfDay(12.0)

-- Spawn one unit at a known location, facing east (90 deg).
Game_CreateUnit(Game_Player(0), footman, { x = 320, y = 256 }, 90)
