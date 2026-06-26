-- Campaign transition hooks for "First Light" (#312). These run in the campaign
-- sandbox between missions, reading the just-finished mission's persisted state
-- from the campaign store and returning the carry-over the engine threads into the
-- next mission. They never touch sim state directly.

local HERO = "Ser Caldus"

-- OnMissionComplete carries Ser Caldus forward from "kindle" into "dawn". The
-- mission persists his final level and whether he still holds the Ember Ward under
-- the campaign category (ctx.category); we read them and build the carry-over,
-- clamping to the demo's minimum guaranteed progression (level 2) so a hero record
-- is always produced even on a fast clear.
function OnMissionComplete(ctx)
  local s = Game_Storage()
  local level, ok = Storage_GetInt(s, ctx.category, "demo:caldus:level")
  if not ok or level < 2 then
    level = 2
  end
  local items = {}
  local ember, emberOK = Storage_GetInt(s, ctx.category, "demo:caldus:ember")
  if emberOK and ember ~= 0 then
    items[#items + 1] = "Ember Ward"
  end
  return {
    next = "dawn",
    heroes = { { name = HERO, level = level, items = items } },
    log = { "kindle complete: " .. HERO .. " carried at level " .. level },
  }
end

-- OnMissionFail loops the player back to retry the gate; no carry-over on a loss.
function OnMissionFail(ctx)
  return { next = "kindle", log = { "kindle failed: regroup at the gate" } }
end
