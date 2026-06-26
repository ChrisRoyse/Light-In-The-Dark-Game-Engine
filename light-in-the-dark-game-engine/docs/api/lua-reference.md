# Lua Scripting API Reference

Generated from `api-manifest.json` (schema v1) by `tools/luadoc`. Do not hand-edit.

410 callable functions. Tombstoned/deferred declarations are intentionally omitted.

## AddAssault

`AddAssault(qty: integer, id: integer) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AICommander.AddAssault` — `(...)`

## AddDefenders

`AddDefenders(qty: integer, id: integer) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AICommander.AddDefenders` — `(...)`

## AddGuardPost

`AddGuardPost(id: integer, x: real, y: real) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.AddGuardPost` — `(...)`

## AddHeroXP

`AddHeroXP(whichHero: unit, xpToAdd: integer, showEyeCandy: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.AddExperience` — `(xp int)`

## AddSpecialEffect

`AddSpecialEffect(modelName: string, x: real, y: real) -> effect`

- Source: `common.j`
- Maps to: `litd/api.Game.AddSpecialEffect` — `(model string, pos Vec2) Effect`

## AngleBetweenPoints

`AngleBetweenPoints(locA: location, locB: location) -> real`

- Source: `blizzard.j`
- Maps to: `litd/api.Vec2.AngleTo` — `(o Vec2) Angle`

## AttackMoveKill

`AttackMoveKill(target: unit) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.AttackMoveKill` — `(...)`

## AttackMoveXY

`AttackMoveXY(x: integer, y: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.AttackMoveXY` — `(...)`

## BlzGetAbilityRealField

`BlzGetAbilityRealField(whichAbility: ability, whichField: abilityrealfield) -> real`

- Source: `common.j`
- Maps to: `litd/api.Ability.Field` — `(field AbilityField) float64`

## BlzGetUnitArmor

`BlzGetUnitArmor(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.Armor` — `() float64`

## BlzGetUnitCollisionSize

`BlzGetUnitCollisionSize(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.CollisionSize` — `() float64`

## BlzGetUnitMaxHP

`BlzGetUnitMaxHP(whichUnit: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.MaxLife` — `() float64`

## BlzGetUnitMaxMana

`BlzGetUnitMaxMana(whichUnit: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.MaxMana` — `() float64`

## BlzIsUnitInvulnerable

`BlzIsUnitInvulnerable(whichUnit: unit) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.Invulnerable` — `() bool`

## BlzSetAbilityRealField

`BlzSetAbilityRealField(whichAbility: ability, whichField: abilityrealfield, value: real) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Ability.SetField` — `(field AbilityField, value float64)`

## BlzSetEventDamage

`BlzSetEventDamage(damage: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.DamageEvent.SetAmount` — `(v float64)`

## BlzSetSpecialEffectColor

`BlzSetSpecialEffectColor(whichEffect: effect, r: integer, g: integer, b: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Effect.SetColor` — `(r, g, b uint8)`

## BlzSetSpecialEffectPosition

`BlzSetSpecialEffectPosition(whichEffect: effect, x: real, y: real, z: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Effect.SetPosition` — `(pos Vec2)`

## BlzSetSpecialEffectScale

`BlzSetSpecialEffectScale(whichEffect: effect, scale: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Effect.SetScale` — `(scale float64)`

## BlzSetUnitMaxHP

`BlzSetUnitMaxHP(whichUnit: unit, hp: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetMaxLife` — `(v float64)`

## BlzSetUnitMaxMana

`BlzSetUnitMaxMana(whichUnit: unit, mana: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetMaxMana` — `(v float64)`

## BlzSetUnitName

`BlzSetUnitName(whichUnit: unit, name: string) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetName` — `(name string)`

## CameraSetSourceNoise

`CameraSetSourceNoise(mag: real, velocity: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Camera.Shake` — `(magnitude float64)`

## CaptainAtGoal

`CaptainAtGoal() -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainAtGoal` — `(...) `

## CaptainAttack

`CaptainAttack(x: real, y: real) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.CaptainAttack` — `(...)`

## CaptainGoHome

`CaptainGoHome() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.CaptainGoHome` — `(...)`

## CaptainGroupSize

`CaptainGroupSize() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainGroupSize` — `(...) `

## CaptainInCombat

`CaptainInCombat(attack_captain: boolean) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainInCombat` — `(...) `

## CaptainIsEmpty

`CaptainIsEmpty() -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainIsEmpty` — `(...) `

## CaptainIsFull

`CaptainIsFull() -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainIsFull` — `(...) `

## CaptainIsHome

`CaptainIsHome() -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainIsHome` — `(...) `

## CaptainReadiness

`CaptainReadiness() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainReadiness` — `(...) `

## CaptainReadinessHP

`CaptainReadinessHP() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainReadinessHP` — `(...) `

## CaptainReadinessMa

`CaptainReadinessMa() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainReadinessMa` — `(...) `

## CaptainRetreating

`CaptainRetreating() -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainRetreating` — `(...) `

## CaptainVsPlayer

`CaptainVsPlayer(id: player) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainVsPlayer` — `(...) `

## CaptainVsUnits

`CaptainVsUnits(id: player) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIView.CaptainVsUnits` — `(...) `

## ChooseRandomItem

`ChooseRandomItem(level: integer) -> integer`

- Source: `common.j`
- Maps to: `litd/api/helpers.RandomItemType` — `(g *Game, codes []string) ItemType`

## ClearCaptainTargets

`ClearCaptainTargets() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.ClearCaptainTargets` — `(...)`

## ClearHarvestAI

`ClearHarvestAI() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.ClearHarvestAI` — `(...)`

## ClearTextMessages

`ClearTextMessages() -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.ClearMessages` — `(to []Player)`

## CommandAI

`CommandAI(num: player, command: integer, data: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.CommandAI` — `(p Player, command, data int)`

## CommandsWaiting

`CommandsWaiting() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AICommander.CommandsWaiting` — `(...)`

## ConvertUnits

`ConvertUnits(qty: integer, id: integer) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.ConvertUnits` — `(...) `

## CountUnitsInGroup

`CountUnitsInGroup(g: group) -> integer`

- Source: `blizzard.j`
- Maps to: `litd/api.UnitSet.Count` — `() int`

## CreateCaptains

`CreateCaptains() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.CreateCaptains` — `(...)`

## CreateDestructable

`CreateDestructable(objectid: integer, x: real, y: real, face: real, scale: real, variation: integer) -> destructable`

- Source: `common.j`
- Maps to: `litd/api.Game.CreateDestructable` — `(o DestructableOptions) Destructable`

## CreateFogModifierRect

`CreateFogModifierRect(forWhichPlayer: player, whichState: fogstate, where: rect, useSharedVision: boolean, afterUnits: boolean) -> fogmodifier`

- Source: `common.j`
- Maps to: `litd/api.Game.NewFogModifier` — `(p Player, state FogState, area Area, opts ...FogOption) FogModifier`

## CreateForce

`CreateForce() -> force`

- Source: `common.j`
- Maps to: `litd/api.Game.CreateForce` — `() Force`

## CreateGroup

`CreateGroup() -> group`

- Source: `common.j`
- Maps to: `litd/api.Game.NewUnitSet` — `() *UnitSet`

## CreateItem

`CreateItem(itemid: integer, x: real, y: real) -> item`

- Source: `common.j`
- Maps to: `litd/api.Game.CreateItem` — `(typ ItemType, pos Vec2) Item`

## CreateNUnitsAtLoc

`CreateNUnitsAtLoc(count: integer, unitId: integer, whichPlayer: player, loc: location, face: real) -> group`

- Source: `blizzard.j`
- Maps to: `litd/api/helpers.CreateUnits` — `(g *Game, n int, owner Player, typ UnitType, pos Vec2, facing Angle) []Unit`

## CreateRegion

`CreateRegion() -> region`

- Source: `common.j`
- Maps to: `litd/api.Game.NewRegion` — `() Region`

## CreateSound

`CreateSound(fileName: string, looping: boolean, is3D: boolean, stopwhenoutofrange: boolean, fadeInRate: integer, fadeOutRate: integer, eaxSetting: string) -> sound`

- Source: `common.j`
- Maps to: `litd/api.Game.CreateSound` — `(cue string) Sound`

## CreateUnit

`CreateUnit(id: player, unitid: integer, x: real, y: real, face: real) -> unit`

- Source: `common.j`
- Maps to: `litd/api.Game.CreateUnit` — `(owner Player, typ UnitType, pos Vec2, facing Angle) Unit`

## CreepsOnMap

`CreepsOnMap() -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.CreepsOnMap` — `(...) `

## CustomDefeatBJ

`CustomDefeatBJ(whichPlayer: player, message: string) -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api.Game.Defeat` — `(p Player, msg string)`

## CustomVictoryBJ

`CustomVictoryBJ(whichPlayer: player, showDialog: boolean, showScores: boolean) -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api.Game.Victory` — `(p Player)`

## DebugFI

`DebugFI(str: string, val: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIThread.DebugFI` — `(...)`

## DebugS

`DebugS(str: string) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIThread.DebugS` — `(...)`

## DebugUnitID

`DebugUnitID(str: string, val: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIThread.DebugUnitID` — `(...)`

## DecUnitAbilityLevel

`DecUnitAbilityLevel(whichUnit: unit, abilcode: integer) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Ability.DecLevel` — `() int`

## Deg2Rad

`Deg2Rad(degrees: real) -> real`

- Source: `common.j`
- Maps to: `litd/api.Deg` — `(degrees float64) Angle`

## DestroyEffect

`DestroyEffect(whichEffect: effect) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Effect.Destroy` — `()`

## DestroyFogModifier

`DestroyFogModifier(whichFogModifier: fogmodifier) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.FogModifier.Destroy` — `()`

## DestroyTimer

`DestroyTimer(whichTimer: timer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Timer.Stop` — `()`

## DestructableRestoreLife

`DestructableRestoreLife(d: destructable, life: real, birth: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Destructable.Resurrect` — `()`

## DisablePathing

`DisablePathing() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.DisablePathing` — `(...)`

## DisplayText

`DisplayText(p: integer, str: string) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIThread.DisplayText` — `(...)`

## DisplayTextI

`DisplayTextI(p: integer, str: string, val: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIThread.DisplayTextI` — `(...)`

## DisplayTextII

`DisplayTextII(p: integer, str: string, v1: integer, v2: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIThread.DisplayTextII` — `(...)`

## DisplayTextIII

`DisplayTextIII(p: integer, str: string, v1: integer, v2: integer, v3: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIThread.DisplayTextIII` — `(...)`

## DisplayTextToPlayer

`DisplayTextToPlayer(toPlayer: player, x: real, y: real, message: string) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.Print` — `(to []Player, msg string, opts ...PrintOption)`

## DistanceBetweenPoints

`DistanceBetweenPoints(locA: location, locB: location) -> real`

- Source: `blizzard.j`
- Maps to: `litd/api.Vec2.DistanceTo` — `(o Vec2) float64`

## DoAiScriptDebug

`DoAiScriptDebug() -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIThread.DoAiScriptDebug` — `(...)`

## EndGame

`EndGame(doScoreScreen: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.EndMatch` — `()`

## FillGuardPosts

`FillGuardPosts() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.FillGuardPosts` — `(...)`

## FlushChildHashtable

`FlushChildHashtable(table: hashtable, parentKey: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Table.RemoveParent` — `(parent int)`

## FlushGameCache

`FlushGameCache(cache: gamecache) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Storage.Clear` — `()`

## FlushParentHashtable

`FlushParentHashtable(table: hashtable) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Table.Clear` — `()`

## FogEnable

`FogEnable(enable: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.SetFogEnabled` — `(on bool)`

## FogMaskEnable

`FogMaskEnable(enable: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.SetFogMaskEnabled` — `(on bool)`

## FogModifierStart

`FogModifierStart(whichFogModifier: fogmodifier) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.FogModifier.Start` — `()`

## FogModifierStop

`FogModifierStop(whichFogModifier: fogmodifier) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.FogModifier.Stop` — `()`

## ForForce

`ForForce(whichForce: force, callback: code) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Force.Players` — `() []Player`

## ForGroup

`ForGroup(whichGroup: group, callback: code) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.UnitSet.Units` — `() []Unit`

## ForceAddPlayer

`ForceAddPlayer(whichForce: force, whichPlayer: player) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Force.AddPlayer` — `(p Player)`

## ForceClear

`ForceClear(whichForce: force) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Force.Clear` — `()`

## ForceEnumPlayers

`ForceEnumPlayers(whichForce: force, filter: boolexpr) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Force.AddAllPlayers` — `()`

## ForceRemovePlayer

`ForceRemovePlayer(whichForce: force, whichPlayer: player) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Force.RemovePlayer` — `(p Player)`

## GetAIDifficulty

`GetAIDifficulty(num: player) -> aidifficulty`

- Source: `common.j`
- Maps to: `litd/api.Game.AIDifficulty` — `(p Player) Difficulty`

## GetAiPlayer

`GetAiPlayer() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.AiPlayer` — `(...) `

## GetAllianceTarget

`GetAllianceTarget() -> unit`

- Source: `commonai`
- Maps to: `litd/api.AIView.AllianceTarget` — `(...) `

## GetBuilding

`GetBuilding(p: player) -> unit`

- Source: `commonai`
- Maps to: `litd/api.AIView.Building` — `(...) `

## GetCameraField

`GetCameraField(whichField: camerafield) -> real`

- Source: `common.j`
- Maps to: `litd/api.Camera.Field` — `(f CameraField) float64`

## GetCreepCamp

`GetCreepCamp(min: integer, max: integer, flyers_ok: boolean) -> unit`

- Source: `commonai`
- Maps to: `litd/api.AIView.CreepCamp` — `(...) `

## GetDestructableLife

`GetDestructableLife(d: destructable) -> real`

- Source: `common.j`
- Maps to: `litd/api.Destructable.Life` — `() int`

## GetDestructableMaxLife

`GetDestructableMaxLife(d: destructable) -> real`

- Source: `common.j`
- Maps to: `litd/api.Destructable.MaxLife` — `() int`

## GetEnemyBase

`GetEnemyBase() -> unit`

- Source: `commonai`
- Maps to: `litd/api.AIView.EnemyBase` — `(...) `

## GetEnemyExpansion

`GetEnemyExpansion() -> unit`

- Source: `commonai`
- Maps to: `litd/api.AIView.EnemyExpansion` — `(...) `

## GetEnemyPower

`GetEnemyPower() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.EnemyPower` — `(...) `

## GetEventDamage

`GetEventDamage() -> real`

- Source: `common.j`
- Maps to: `litd/api.Event.Damage` — `() float64`

## GetEventDamageSource

`GetEventDamageSource() -> unit`

- Source: `common.j`
- Maps to: `litd/api.Event.Source` — `() Unit`

## GetExpansionFoe

`GetExpansionFoe() -> unit`

- Source: `commonai`
- Maps to: `litd/api.AIView.ExpansionFoe` — `(...) `

## GetExpansionPeon

`GetExpansionPeon() -> unit`

- Source: `commonai`
- Maps to: `litd/api.AIView.ExpansionPeon` — `(...) `

## GetExpansionX

`GetExpansionX() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.ExpansionX` — `(...) `

## GetExpansionY

`GetExpansionY() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.ExpansionY` — `(...) `

## GetGoldOwned

`GetGoldOwned() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.GoldOwned` — `(...) `

## GetHandleId

`GetHandleId(h: handle) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.ID` — `() uint32`

## GetHeroAgi

`GetHeroAgi(whichHero: unit, includeBonuses: boolean) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.Agility` — `() int`

## GetHeroId

`GetHeroId() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.HeroId` — `(...) `

## GetHeroInt

`GetHeroInt(whichHero: unit, includeBonuses: boolean) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.Intelligence` — `() int`

## GetHeroLevel

`GetHeroLevel(whichHero: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.HeroLevel` — `() int`

## GetHeroLevelAI

`GetHeroLevelAI() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.HeroLevelAI` — `(...) `

## GetHeroSkillPoints

`GetHeroSkillPoints(whichHero: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.SkillPoints` — `() int`

## GetHeroStr

`GetHeroStr(whichHero: unit, includeBonuses: boolean) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.Strength` — `() int`

## GetHeroXP

`GetHeroXP(whichHero: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.HeroXP` — `() int`

## GetItemCharges

`GetItemCharges(whichItem: item) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Item.Charges` — `() int`

## GetItemTypeId

`GetItemTypeId(i: item) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Item.Type` — `() ItemType`

## GetItemX

`GetItemX(i: item) -> real`

- Source: `common.j`
- Maps to: `litd/api.Item.Position` — `() Vec2`

## GetKillingUnit

`GetKillingUnit() -> unit`

- Source: `common.j`
- Maps to: `litd/api.Event.KillingUnit` — `() Unit`

## GetLastCommand

`GetLastCommand() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AICommander.LastCommand` — `(...)`

## GetLastData

`GetLastData() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AICommander.LastData` — `(...)`

## GetLocationZ

`GetLocationZ(whichLocation: location) -> real`

- Source: `common.j`
- Maps to: `litd/api.Game.TerrainHeight` — `(p Vec2) float64`

## GetMegaTarget

`GetMegaTarget() -> unit`

- Source: `commonai`
- Maps to: `litd/api.AIView.MegaTarget` — `(...) `

## GetMinesOwned

`GetMinesOwned() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.MinesOwned` — `(...) `

## GetNextExpansion

`GetNextExpansion() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.NextExpansion` — `(...) `

## GetOrderTargetUnit

`GetOrderTargetUnit() -> unit`

- Source: `common.j`
- Maps to: `litd/api.Event.Target` — `() Unit`

## GetOwningPlayer

`GetOwningPlayer(whichUnit: unit) -> player`

- Source: `common.j`
- Maps to: `litd/api.Unit.Owner` — `() Player`

## GetPlayerAlliance

`GetPlayerAlliance(sourcePlayer: player, otherPlayer: player, whichAllianceSetting: alliancetype) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Player.AllianceWith` — `(other Player) AllianceFlags`

## GetPlayerColor

`GetPlayerColor(whichPlayer: player) -> playercolor`

- Source: `common.j`
- Maps to: `litd/api.Player.Color` — `() int`

## GetPlayerController

`GetPlayerController(whichPlayer: player) -> mapcontrol`

- Source: `common.j`
- Maps to: `litd/api.Player.Controller` — `() Controller`

## GetPlayerHandicap

`GetPlayerHandicap(whichPlayer: player) -> real`

- Source: `common.j`
- Maps to: `litd/api.Player.Handicap` — `() float64`

## GetPlayerHandicapDamage

`GetPlayerHandicapDamage(whichPlayer: player) -> real`

- Source: `common.j`
- Maps to: `litd/api.Player.HandicapDamage` — `() float64`

## GetPlayerHandicapReviveTime

`GetPlayerHandicapReviveTime(whichPlayer: player) -> real`

- Source: `common.j`
- Maps to: `litd/api.Player.HandicapReviveTime` — `() float64`

## GetPlayerHandicapXP

`GetPlayerHandicapXP(whichPlayer: player) -> real`

- Source: `common.j`
- Maps to: `litd/api.Player.HandicapXP` — `() float64`

## GetPlayerId

`GetPlayerId(whichPlayer: player) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Player.Slot` — `() int`

## GetPlayerName

`GetPlayerName(whichPlayer: player) -> string`

- Source: `common.j`
- Maps to: `litd/api.Player.Name` — `() string`

## GetPlayerNeutralAggressive

`GetPlayerNeutralAggressive() -> integer`

- Source: `common.j`
- Maps to: `litd/api.Game.NeutralHostile` — `() Player`

## GetPlayerNeutralPassive

`GetPlayerNeutralPassive() -> integer`

- Source: `common.j`
- Maps to: `litd/api.Game.NeutralPassive` — `() Player`

## GetPlayerRace

`GetPlayerRace(whichPlayer: player) -> race`

- Source: `common.j`
- Maps to: `litd/api.Player.Race` — `() Race`

## GetPlayerStartLocation

`GetPlayerStartLocation(whichPlayer: player) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Player.StartLocation` — `() Vec2`

## GetPlayerState

`GetPlayerState(whichPlayer: player, whichPlayerState: playerstate) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Player.Gold` — `() int`

## GetPlayerTaxRate

`GetPlayerTaxRate(sourcePlayer: player, otherPlayer: player, whichResource: playerstate) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Player.TaxRate` — `(other Player, resource int) float64`

## GetPlayerTeam

`GetPlayerTeam(whichPlayer: player) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Player.Team` — `() int`

## GetPlayerUnitTypeCount

`GetPlayerUnitTypeCount(p: player, unitid: integer) -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.PlayerUnitTypeCount` — `(...) `

## GetPlayersAllies

`GetPlayersAllies(whichPlayer: player) -> force`

- Source: `blizzard.j`
- Maps to: `litd/api.Game.Allies` — `(p Player) []Player`

## GetPlayersEnemies

`GetPlayersEnemies(whichPlayer: player) -> force`

- Source: `blizzard.j`
- Maps to: `litd/api.Game.Enemies` — `(p Player) []Player`

## GetPlayersMatching

`GetPlayersMatching(filter: boolexpr) -> force`

- Source: `blizzard.j`
- Maps to: `litd/api.Game.Players` — `(filter PlayerFilter) []Player`

## GetRandomDirectionDeg

`GetRandomDirectionDeg() -> real`

- Source: `blizzard.j`
- Maps to: `litd/api.Game.RandomAngle` — `() Angle`

## GetRandomInt

`GetRandomInt(lowBound: integer, highBound: integer) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Game.RandomInt` — `(min, max int) int`

## GetRandomLocInRect

`GetRandomLocInRect(whichRect: rect) -> location`

- Source: `blizzard.j`
- Maps to: `litd/api.Game.RandomPointIn` — `(rc Rect) Vec2`

## GetRandomReal

`GetRandomReal(lowBound: real, highBound: real) -> real`

- Source: `common.j`
- Maps to: `litd/api.Game.RandomFloat` — `() float64`

## GetRectCenterX

`GetRectCenterX(whichRect: rect) -> real`

- Source: `common.j`
- Maps to: `litd/api.Rect.Center` — `() Vec2`

## GetRectHeightBJ

`GetRectHeightBJ(r: rect) -> real`

- Source: `blizzard.j`
- Maps to: `litd/api.Rect.Height` — `() float64`

## GetRectMaxX

`GetRectMaxX(whichRect: rect) -> real`

- Source: `common.j`
- Maps to: `litd/api.Rect.Max` — `() Vec2`

## GetRectMinX

`GetRectMinX(whichRect: rect) -> real`

- Source: `common.j`
- Maps to: `litd/api.Rect.Min` — `() Vec2`

## GetRectWidthBJ

`GetRectWidthBJ(r: rect) -> real`

- Source: `blizzard.j`
- Maps to: `litd/api.Rect.Width` — `() float64`

## GetStartLocationX

`GetStartLocationX(whichStartLocation: integer) -> real`

- Source: `common.j`
- Maps to: `litd/api.Game.StartLocation` — `(i int) Vec2`

## GetStoredInteger

`GetStoredInteger(cache: gamecache, missionKey: string, key: string) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Storage.GetInt` — `(category, key string) (int, bool)`

## GetTeams

`GetTeams() -> integer`

- Source: `common.j`
- Maps to: `litd/api.Game.Teams` — `() int`

## GetTimeOfDay

`GetTimeOfDay() -> real`

- Source: `blizzard.j`
- Maps to: `litd/api.Game.TimeOfDay` — `() float64`

## GetTimeOfDayScale

`GetTimeOfDayScale() -> real`

- Source: `common.j`
- Maps to: `litd/api.Game.TimeOfDayScale` — `() float64`

## GetTownUnitCount

`GetTownUnitCount(id: integer, tn: integer, dn: boolean) -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.TownUnitCount` — `(...) `

## GetTriggerPlayer

`GetTriggerPlayer() -> player`

- Source: `common.j`
- Maps to: `litd/api.Event.Player` — `() Player`

## GetTriggerUnit

`GetTriggerUnit() -> unit`

- Source: `common.j`
- Maps to: `litd/api.Event.Unit` — `() Unit`

## GetTriggeringRegion

`GetTriggeringRegion() -> region`

- Source: `common.j`
- Maps to: `litd/api.Event.Region` — `() Region`

## GetTriggeringTrigger

`GetTriggeringTrigger() -> trigger`

- Source: `common.j`
- Maps to: `litd/api.Event.Subscription` — `() Subscription`

## GetUnitAbilityLevel

`GetUnitAbilityLevel(whichUnit: unit, abilcode: integer) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Ability.Level` — `() int`

## GetUnitAcquireRange

`GetUnitAcquireRange(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.AcquireRange` — `() float64`

## GetUnitBuildTime

`GetUnitBuildTime(unitid: integer) -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.UnitBuildTime` — `(...) `

## GetUnitCount

`GetUnitCount(unitid: integer) -> integer`

- Source: `commonai`
- Maps to: `litd/ai.UnitCount` — `(unitTypeID int) int`

## GetUnitCountDone

`GetUnitCountDone(unitid: integer) -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.UnitCountDone` — `(...) `

## GetUnitCurrentOrder

`GetUnitCurrentOrder(whichUnit: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.CurrentOrder` — `() Order`

## GetUnitDefaultAcquireRange

`GetUnitDefaultAcquireRange(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.DefaultAcquireRange` — `() float64`

## GetUnitDefaultFlyHeight

`GetUnitDefaultFlyHeight(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.DefaultFlyHeight` — `() float64`

## GetUnitDefaultMoveSpeed

`GetUnitDefaultMoveSpeed(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.DefaultMoveSpeed` — `() float64`

## GetUnitDefaultPropWindow

`GetUnitDefaultPropWindow(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.DefaultPropWindow` — `() float64`

## GetUnitDefaultTurnSpeed

`GetUnitDefaultTurnSpeed(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.DefaultTurnSpeed` — `() float64`

## GetUnitFacing

`GetUnitFacing(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.Facing` — `() Angle`

## GetUnitFlyHeight

`GetUnitFlyHeight(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.FlyHeight` — `() float64`

## GetUnitFoodMade

`GetUnitFoodMade(whichUnit: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.FoodMade` — `() int`

## GetUnitFoodUsed

`GetUnitFoodUsed(whichUnit: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.FoodUsed` — `() int`

## GetUnitGoldCost

`GetUnitGoldCost(unitid: integer) -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.UnitGoldCost` — `(...) `

## GetUnitLevel

`GetUnitLevel(whichUnit: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.Level` — `() int`

## GetUnitLifePercent

`GetUnitLifePercent(whichUnit: unit) -> real`

- Source: `blizzard.j`
- Maps to: `litd/api.Unit.LifePercent` — `() float64`

## GetUnitManaPercent

`GetUnitManaPercent(whichUnit: unit) -> real`

- Source: `blizzard.j`
- Maps to: `litd/api.Unit.ManaPercent` — `() float64`

## GetUnitMoveSpeed

`GetUnitMoveSpeed(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.MoveSpeed` — `() float64`

## GetUnitName

`GetUnitName(whichUnit: unit) -> string`

- Source: `common.j`
- Maps to: `litd/api.Unit.Name` — `() string`

## GetUnitPointValue

`GetUnitPointValue(whichUnit: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.PointValue` — `() int`

## GetUnitPropWindow

`GetUnitPropWindow(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.PropWindow` — `() float64`

## GetUnitRace

`GetUnitRace(whichUnit: unit) -> race`

- Source: `common.j`
- Maps to: `litd/api.Unit.Race` — `() Race`

## GetUnitRallyPoint

`GetUnitRallyPoint(whichUnit: unit) -> location`

- Source: `common.j`
- Maps to: `litd/api.Unit.RallyPoint` — `() Vec2`

## GetUnitRallyUnit

`GetUnitRallyUnit(whichUnit: unit) -> unit`

- Source: `common.j`
- Maps to: `litd/api.Unit.RallyUnit` — `() Unit`

## GetUnitState

`GetUnitState(whichUnit: unit, whichUnitState: unitstate) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.Life` — `() float64`

## GetUnitTurnSpeed

`GetUnitTurnSpeed(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.TurnSpeed` — `() float64`

## GetUnitTypeId

`GetUnitTypeId(whichUnit: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.Type` — `() UnitType`

## GetUnitUserData

`GetUnitUserData(whichUnit: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.UserData` — `() int`

## GetUnitWoodCost

`GetUnitWoodCost(unitid: integer) -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.UnitWoodCost` — `(...) `

## GetUnitX

`GetUnitX(whichUnit: unit) -> real`

- Source: `common.j`
- Maps to: `litd/api.Unit.Position` — `() Vec2`

## GetUpgradeGoldCost

`GetUpgradeGoldCost(id: integer) -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.UpgradeGoldCost` — `(...) `

## GetUpgradeLevel

`GetUpgradeLevel(id: integer) -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.UpgradeLevel` — `(...) `

## GetUpgradeWoodCost

`GetUpgradeWoodCost(id: integer) -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.UpgradeWoodCost` — `(...) `

## GetWorldBounds

`GetWorldBounds() -> rect`

- Source: `common.j`
- Maps to: `litd/api.Game.WorldBounds` — `() Rect`

## GroupAddUnit

`GroupAddUnit(whichGroup: group, whichUnit: unit) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.UnitSet.Add` — `(u Unit)`

## GroupClear

`GroupClear(whichGroup: group) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.UnitSet.Clear` — `()`

## GroupEnumUnitsInRange

`GroupEnumUnitsInRange(whichGroup: group, x: real, y: real, radius: real, filter: boolexpr) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.UnitsInRange` — `(pos Vec2, r float64, filter UnitFilter) []Unit`

## GroupEnumUnitsInRect

`GroupEnumUnitsInRect(whichGroup: group, r: rect, filter: boolexpr) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.UnitsIn` — `(rect Rect, filter UnitFilter) []Unit`

## GroupEnumUnitsOfPlayer

`GroupEnumUnitsOfPlayer(whichGroup: group, whichPlayer: player, filter: boolexpr) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.AllUnits` — `(filter UnitFilter) []Unit`

## GroupRemoveUnit

`GroupRemoveUnit(whichGroup: group, whichUnit: unit) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.UnitSet.Remove` — `(u Unit)`

## GroupTimedLife

`GroupTimedLife(allow: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.GroupTimedLife` — `(...)`

## HarvestGold

`HarvestGold(town: integer, peons: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.HarvestGold` — `(...)`

## HarvestWood

`HarvestWood(town: integer, peons: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.HarvestWood` — `(...)`

## HaveSavedInteger

`HaveSavedInteger(table: hashtable, parentKey: integer, childKey: integer) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Table.Has` — `(parent, child int) bool`

## IgnoredUnits

`IgnoredUnits(unitid: integer) -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.IgnoredUnits` — `(...) `

## IncUnitAbilityLevel

`IncUnitAbilityLevel(whichUnit: unit, abilcode: integer) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Ability.IncLevel` — `() int`

## InitAssault

`InitAssault() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.InitAssault` — `(...)`

## InitGameCache

`InitGameCache(campaignFile: string) -> gamecache`

- Source: `common.j`
- Maps to: `litd/api.Game.Storage` — `() *Storage`

## InitHashtable

`InitHashtable() -> hashtable`

- Source: `common.j`
- Maps to: `litd/api.NewTable` — `[V any]() *Table[V]`

## IsDestructableDeadBJ

`IsDestructableDeadBJ(d: destructable) -> boolean`

- Source: `blizzard.j`
- Maps to: `litd/api.Destructable.Dead` — `() bool`

## IsDestructableInvulnerable

`IsDestructableInvulnerable(d: destructable) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Destructable.Invulnerable` — `() bool`

## IsFogEnabled

`IsFogEnabled() -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Game.FogEnabled` — `() bool`

## IsFogMaskEnabled

`IsFogMaskEnabled() -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Game.FogMaskEnabled` — `() bool`

## IsFoggedToPlayer

`IsFoggedToPlayer(x: real, y: real, whichPlayer: player) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Game.FogStateAt` — `(p Player, pos Vec2) FogState`

## IsMapFlagSet

`IsMapFlagSet(whichMapFlag: mapflag) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Game.MapFlag` — `(f MapFlag) bool`

## IsPlayerAlly

`IsPlayerAlly(whichPlayer: player, otherPlayer: player) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Player.IsAlly` — `(other Player) bool`

## IsPlayerEnemy

`IsPlayerEnemy(whichPlayer: player, otherPlayer: player) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Player.IsEnemy` — `(other Player) bool`

## IsPlayerInForce

`IsPlayerInForce(whichPlayer: player, whichForce: force) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Force.Contains` — `(p Player) bool`

## IsPointInRegion

`IsPointInRegion(whichRegion: region, x: real, y: real) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Region.Contains` — `(p Vec2) bool`

## IsSuspendedXP

`IsSuspendedXP(whichHero: unit) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.ExperienceSuspended` — `() bool`

## IsTowered

`IsTowered(target: unit) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.Towered` — `(...) `

## IsUnitAliveBJ

`IsUnitAliveBJ(whichUnit: unit) -> boolean`

- Source: `blizzard.j`
- Maps to: `litd/api.Unit.Alive` — `() bool`

## IsUnitHidden

`IsUnitHidden(whichUnit: unit) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.IsHidden` — `() bool`

## IsUnitInGroup

`IsUnitInGroup(whichUnit: unit, whichGroup: group) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.UnitSet.Contains` — `(u Unit) bool`

## IsUnitInRange

`IsUnitInRange(whichUnit: unit, otherUnit: unit, distance: real) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.InRange` — `(other Unit, distance float64) bool`

## IsUnitInRangeXY

`IsUnitInRangeXY(whichUnit: unit, x: real, y: real, distance: real) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.InRangeOf` — `(point Vec2, distance float64) bool`

## IsUnitInRegion

`IsUnitInRegion(whichRegion: region, whichUnit: unit) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Region.ContainsUnit` — `(u Unit) bool`

## IsUnitOwnedByPlayer

`IsUnitOwnedByPlayer(whichUnit: unit, whichPlayer: player) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.OwnedBy` — `(p Player) bool`

## IsUnitPausedBJ

`IsUnitPausedBJ(whichUnit: unit) -> boolean`

- Source: `blizzard.j`
- Maps to: `litd/api.Unit.Paused` — `() bool`

## IsUnitRace

`IsUnitRace(whichUnit: unit, whichRace: race) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.IsRace` — `(r Race) bool`

## IsUnitType

`IsUnitType(whichUnit: unit, whichUnitType: unittype) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.IsType` — `(class UnitClass) bool`

## IsUnitVisible

`IsUnitVisible(whichUnit: unit, whichPlayer: player) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.VisibleTo` — `(p Player) bool`

## IsVisibleToPlayer

`IsVisibleToPlayer(x: real, y: real, whichPlayer: player) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Game.IsVisibleTo` — `(p Player, pos Vec2) bool`

## IssuePointOrder

`IssuePointOrder(whichUnit: unit, order: string, x: real, y: real) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.Order` — `(ord Order, target OrderTarget) bool`

## IssueTrainOrderByIdBJ

`IssueTrainOrderByIdBJ(whichUnit: unit, unitId: integer) -> boolean`

- Source: `blizzard.j`
- Maps to: `litd/api.Unit.Train` — `(typ UnitType) bool`

## KillDestructable

`KillDestructable(d: destructable) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Destructable.Kill` — `()`

## KillUnit

`KillUnit(whichUnit: unit) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.Kill` — `()`

## LoadInteger

`LoadInteger(table: hashtable, parentKey: integer, childKey: integer) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Table.Get` — `(parent, child int) (V, bool)`

## LoadZepWave

`LoadZepWave(x: integer, y: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.LoadZepWave` — `(...)`

## MeleeCheckAddedUnit

`MeleeCheckAddedUnit(addedUnit: unit) -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api/helpers/melee.melee.Standard` — `(g *litd.Game, setups []Setup) error`

## MeleeInitVictoryDefeat

`MeleeInitVictoryDefeat() -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api/helpers/melee.melee.VictoryDefeatConditions` — `(g *litd.Game, players []litd.Player)`

## MeleeStartingResources

`MeleeStartingResources() -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api/helpers/melee.melee.StartingResources` — `(g *litd.Game, p litd.Player, f *Faction)`

## MeleeStartingUnits

`MeleeStartingUnits() -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api/helpers/melee.melee.StartingUnits` — `(g *litd.Game, p litd.Player, f *Faction) ([]litd.Unit, error)`

## MergeUnits

`MergeUnits(qty: integer, a: integer, b: integer, make: integer) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AICommander.MergeUnits` — `(...)`

## MoveRectTo

`MoveRectTo(whichRect: rect, newCenterX: real, newCenterY: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Rect.Offset` — `(d Vec2) Rect`

## OffsetLocation

`OffsetLocation(loc: location, dx: real, dy: real) -> location`

- Source: `blizzard.j`
- Maps to: `litd/api.Vec2.Add` — `(o Vec2) Vec2`

## PanCameraTo

`PanCameraTo(x: real, y: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Camera.Pan` — `(pos Vec2, opts ...PanOption)`

## PauseCompAI

`PauseCompAI(p: player, pause: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.PauseAI` — `(p Player, paused bool)`

## PauseGame

`PauseGame(flag: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.Pause` — `()`

## PauseTimer

`PauseTimer(whichTimer: timer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Timer.Pause` — `()`

## PauseUnitBJ

`PauseUnitBJ(pause: boolean, whichUnit: unit) -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api.Unit.SetPaused` — `(paused bool)`

## PlayMusic

`PlayMusic(musicName: string) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.PlayMusic` — `(cue string)`

## PlaySoundAtPointBJ

`PlaySoundAtPointBJ(soundHandle: sound, volumePercent: real, loc: location, z: real) -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api.Sound.PlayAt` — `(pos Vec2, z float64)`

## PlaySoundOnUnitBJ

`PlaySoundOnUnitBJ(soundHandle: sound, volumePercent: real, whichUnit: unit) -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api.Sound.PlayOn` — `(u Unit)`

## Player

`Player(number: integer) -> player`

- Source: `common.j`
- Maps to: `litd/api.Game.Player` — `(slot int) Player`

## PolarProjectionBJ

`PolarProjectionBJ(source: location, dist: real, angle: real) -> location`

- Source: `blizzard.j`
- Maps to: `litd/api.Vec2.Polar` — `(a Angle, dist float64) Vec2`

## PolledWait

`PolledWait(duration: real) -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api/helpers.PolledWait` — `(seconds float64)`

## PopLastCommand

`PopLastCommand() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.PopCommand` — `(...)`

## PurchaseZeppelin

`PurchaseZeppelin() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.PurchaseZeppelin` — `(...)`

## QueueDestructableAnimation

`QueueDestructableAnimation(d: destructable, whichAnimation: string) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Destructable.PlayAnimation` — `(name string)`

## Rad2Deg

`Rad2Deg(radians: real) -> real`

- Source: `common.j`
- Maps to: `litd/api.Angle.Degrees` — `() float64`

## RandomDistChoose

`RandomDistChoose() -> integer`

- Source: `blizzard.j`
- Maps to: `litd/api/helpers.WeightedChoice` — `(g *Game, weights []int) int`

## Rect

`Rect(minx: real, miny: real, maxx: real, maxy: real) -> rect`

- Source: `common.j`
- Maps to: `litd/api.NewRect` — `(a, b Vec2) Rect`

## RectContainsCoords

`RectContainsCoords(r: rect, x: real, y: real) -> boolean`

- Source: `blizzard.j`
- Maps to: `litd/api.Rect.Contains` — `(p Vec2) bool`

## RectFromCenterSizeBJ

`RectFromCenterSizeBJ(center: location, width: real, height: real) -> rect`

- Source: `blizzard.j`
- Maps to: `litd/api.RectAround` — `(c Vec2, w, h float64) Rect`

## RegionAddCell

`RegionAddCell(whichRegion: region, x: real, y: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Region.AddCell` — `(p Vec2)`

## RegionAddRect

`RegionAddRect(whichRegion: region, r: rect) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Region.AddRect` — `(rc Rect)`

## RegionClearCell

`RegionClearCell(whichRegion: region, x: real, y: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Region.RemoveCell` — `(p Vec2)`

## RegionClearRect

`RegionClearRect(whichRegion: region, r: rect) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Region.RemoveRect` — `(rc Rect)`

## RemoveInjuries

`RemoveInjuries() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.RemoveInjuries` — `(...)`

## RemoveItem

`RemoveItem(whichItem: item) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Item.Remove` — `()`

## RemoveRegion

`RemoveRegion(whichRegion: region) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Region.Remove` — `()`

## RemoveSavedInteger

`RemoveSavedInteger(table: hashtable, parentKey: integer, childKey: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Table.Remove` — `(parent, child int)`

## RemoveSiege

`RemoveSiege() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.RemoveSiege` — `(...)`

## RemoveUnit

`RemoveUnit(whichUnit: unit) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.Remove` — `()`

## ResetCaptainLocs

`ResetCaptainLocs() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.ResetCaptainLocs` — `(...)`

## ResumeTimer

`ResumeTimer(whichTimer: timer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Timer.Resume` — `()`

## ReturnGuardPosts

`ReturnGuardPosts() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.ReturnGuardPosts` — `(...)`

## SaveInteger

`SaveInteger(table: hashtable, parentKey: integer, childKey: integer, value: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Table.Set` — `(parent, child int, v V)`

## SelectHeroSkill

`SelectHeroSkill(whichHero: unit, abilcode: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.LearnSkill` — `(skill int) bool`

## SetAllianceTarget

`SetAllianceTarget(id: unit) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetAllianceTarget` — `(...)`

## SetAmphibious

`SetAmphibious() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetAmphibious` — `(...)`

## SetCameraBounds

`SetCameraBounds(x1: real, y1: real, x2: real, y2: real, x3: real, y3: real, x4: real, y4: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Camera.SetBounds` — `(r Rect)`

## SetCameraField

`SetCameraField(whichField: camerafield, value: real, duration: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Camera.SetField` — `(f CameraField, v float64)`

## SetCameraTargetController

`SetCameraTargetController(whichUnit: unit, xoffset: real, yoffset: real, inheritOrientation: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Camera.Follow` — `(u Unit)`

## SetCaptainChanges

`SetCaptainChanges(allow: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetCaptainChanges` — `(...)`

## SetCaptainHome

`SetCaptainHome(which: integer, x: real, y: real) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetCaptainHome` — `(...)`

## SetDefendPlayer

`SetDefendPlayer(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetDefendPlayer` — `(...)`

## SetDestructableInvulnerable

`SetDestructableInvulnerable(d: destructable, flag: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Destructable.SetInvulnerable` — `(b bool)`

## SetDestructableLife

`SetDestructableLife(d: destructable, life: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Destructable.SetLife` — `(v int)`

## SetDoodadAnimation

`SetDoodadAnimation(x: real, y: real, radius: real, doodadID: integer, nearestOnly: boolean, animName: string, animRandom: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Doodad.SetAnimation` — `(anim int)`

## SetExpansion

`SetExpansion(peon: unit, id: integer) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetExpansion` — `(...)`

## SetFogStateRect

`SetFogStateRect(forWhichPlayer: player, whichState: fogstate, where: rect, useSharedVision: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.SetFogState` — `(p Player, state FogState, area Area, sharedVision bool)`

## SetGameSpeed

`SetGameSpeed(whichspeed: gamespeed) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.SetSpeed` — `(s GameSpeed)`

## SetGroupsFlee

`SetGroupsFlee(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetGroupsFlee` — `(...)`

## SetHeroAgi

`SetHeroAgi(whichHero: unit, newAgi: integer, permanent: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetAgility` — `(v int)`

## SetHeroInt

`SetHeroInt(whichHero: unit, newInt: integer, permanent: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetIntelligence` — `(v int)`

## SetHeroLevel

`SetHeroLevel(whichHero: unit, level: integer, showEyeCandy: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetHeroLevel` — `(level int)`

## SetHeroLevels

`SetHeroLevels(func: code) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetHeroLevels` — `(...)`

## SetHeroStr

`SetHeroStr(whichHero: unit, newStr: integer, permanent: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetStrength` — `(v int)`

## SetHeroXP

`SetHeroXP(whichHero: unit, newXpVal: integer, showEyeCandy: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetHeroXP` — `(xp int)`

## SetHeroesBuyItems

`SetHeroesBuyItems(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetHeroesBuyItems` — `(...)`

## SetHeroesFlee

`SetHeroesFlee(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetHeroesFlee` — `(...)`

## SetHeroesTakeItems

`SetHeroesTakeItems(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetHeroesTakeItems` — `(...)`

## SetIgnoreInjured

`SetIgnoreInjured(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetIgnoreInjured` — `(...)`

## SetItemCharges

`SetItemCharges(whichItem: item, charges: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Item.SetCharges` — `(n int)`

## SetMusicVolume

`SetMusicVolume(volume: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.SetMusicVolume` — `(v float64)`

## SetNewHeroes

`SetNewHeroes(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetNewHeroes` — `(...)`

## SetPeonsRepair

`SetPeonsRepair(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetPeonsRepair` — `(...)`

## SetPlayerAlliance

`SetPlayerAlliance(sourcePlayer: player, otherPlayer: player, whichAllianceSetting: alliancetype, value: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetAllianceFlag` — `(other Player, flag AllianceFlags, on bool)`

## SetPlayerColor

`SetPlayerColor(whichPlayer: player, color: playercolor) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetColor` — `(c int)`

## SetPlayerController

`SetPlayerController(whichPlayer: player, controlType: mapcontrol) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetController` — `(c Controller)`

## SetPlayerHandicap

`SetPlayerHandicap(whichPlayer: player, handicap: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetHandicap` — `(v float64)`

## SetPlayerHandicapDamage

`SetPlayerHandicapDamage(whichPlayer: player, handicap: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetHandicapDamage` — `(v float64)`

## SetPlayerHandicapReviveTime

`SetPlayerHandicapReviveTime(whichPlayer: player, handicap: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetHandicapReviveTime` — `(v float64)`

## SetPlayerHandicapXP

`SetPlayerHandicapXP(whichPlayer: player, handicap: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetHandicapXP` — `(v float64)`

## SetPlayerName

`SetPlayerName(whichPlayer: player, name: string) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetName` — `(s string)`

## SetPlayerRacePreference

`SetPlayerRacePreference(whichPlayer: player, whichRacePreference: racepreference) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetRace` — `(r Race)`

## SetPlayerStartLocation

`SetPlayerStartLocation(whichPlayer: player, startLocIndex: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetStartLocation` — `(loc Vec2)`

## SetPlayerState

`SetPlayerState(whichPlayer: player, whichPlayerState: playerstate, value: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetGold` — `(v int)`

## SetPlayerTaxRate

`SetPlayerTaxRate(sourcePlayer: player, otherPlayer: player, whichResource: playerstate, rate: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetTaxRate` — `(other Player, resource int, rate float64)`

## SetPlayerTeam

`SetPlayerTeam(whichPlayer: player, whichTeam: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Player.SetTeam` — `(t int)`

## SetProduce

`SetProduce(qty: integer, id: integer, town: integer) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetProduce` — `(...)`

## SetRandomPaths

`SetRandomPaths(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetRandomPaths` — `(...)`

## SetRandomSeed

`SetRandomSeed(seed: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.SetRandomSeed` — `(seed int64)`

## SetReplacementCount

`SetReplacementCount(qty: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetReplacementCount` — `(...)`

## SetRescueBuildingColorChangeBJ

`SetRescueBuildingColorChangeBJ(changeColor: boolean) -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api.AICommander.SetRescueBuildingColorChangeBJ` — `(...)`

## SetSlowChopping

`SetSlowChopping(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetSlowChopping` — `(...)`

## SetSmartArtillery

`SetSmartArtillery(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetSmartArtillery` — `(...)`

## SetSoundPitch

`SetSoundPitch(soundHandle: sound, pitch: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Sound.SetPitch` — `(p float64)`

## SetSoundVolume

`SetSoundVolume(soundHandle: sound, volume: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Sound.SetVolume` — `(v float64)`

## SetStagePoint

`SetStagePoint(x: real, y: real) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetStagePoint` — `(...)`

## SetTargetHeroes

`SetTargetHeroes(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetTargetHeroes` — `(...)`

## SetTimeOfDay

`SetTimeOfDay(whatTime: real) -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api.Game.SetTimeOfDay` — `(h float64)`

## SetTimeOfDayScale

`SetTimeOfDayScale(r: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.SetTimeOfDayScale` — `(s float64)`

## SetUnitAbilityLevel

`SetUnitAbilityLevel(whichUnit: unit, abilcode: integer, level: integer) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Ability.SetLevel` — `(level int)`

## SetUnitAcquireRange

`SetUnitAcquireRange(whichUnit: unit, newAcquireRange: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetAcquireRange` — `(v float64)`

## SetUnitFacing

`SetUnitFacing(whichUnit: unit, facingAngle: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetFacing` — `(a Angle)`

## SetUnitFlyHeight

`SetUnitFlyHeight(whichUnit: unit, newHeight: real, rate: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetFlyHeight` — `(newHeight, ratePerSec float64)`

## SetUnitInvulnerable

`SetUnitInvulnerable(whichUnit: unit, flag: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetInvulnerable` — `(on bool)`

## SetUnitManaBJ

`SetUnitManaBJ(whichUnit: unit, newValue: real) -> nothing`

- Source: `blizzard.j`
- Maps to: `litd/api.Unit.SetMana` — `(v float64)`

## SetUnitMoveSpeed

`SetUnitMoveSpeed(whichUnit: unit, newSpeed: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetMoveSpeed` — `(v float64)`

## SetUnitOwner

`SetUnitOwner(whichUnit: unit, whichPlayer: player, changeColor: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetOwner` — `(p Player, changeColor bool)`

## SetUnitPosition

`SetUnitPosition(whichUnit: unit, newX: real, newY: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetPosition` — `(pos Vec2)`

## SetUnitPropWindow

`SetUnitPropWindow(whichUnit: unit, newPropWindowAngle: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetPropWindow` — `(radians float64)`

## SetUnitState

`SetUnitState(whichUnit: unit, whichUnitState: unitstate, newVal: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetLife` — `(v float64)`

## SetUnitTurnSpeed

`SetUnitTurnSpeed(whichUnit: unit, newTurnSpeed: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetTurnSpeed` — `(radPerSec float64)`

## SetUnitUserData

`SetUnitUserData(whichUnit: unit, data: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SetUserData` — `(v int)`

## SetUnitsFlee

`SetUnitsFlee(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetUnitsFlee` — `(...)`

## SetUpgrade

`SetUpgrade(id: integer) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetUpgrade` — `(...)`

## SetWatchMegaTargets

`SetWatchMegaTargets(state: boolean) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SetWatchMegaTargets` — `(...)`

## ShiftTownSpot

`ShiftTownSpot(x: real, y: real) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.ShiftTownSpot` — `(...)`

## ShowUnit

`ShowUnit(whichUnit: unit, show: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.Show` — `(show bool)`

## Sleep

`Sleep(seconds: real) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIThread.Sleep` — `(...)`

## StartGetEnemyBase

`StartGetEnemyBase() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIThread.StartGetEnemyBase` — `(...)`

## StartMeleeAI

`StartMeleeAI(num: player, script: string) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.AttachAI` — `(p Player, ai AIController, d Difficulty)`

## StartSound

`StartSound(soundHandle: sound) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Sound.Play` — `()`

## StartThread

`StartThread(func: code) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AIThread.Start` — `(...)`

## StopCamera

`StopCamera() -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Camera.StopFollow` — `()`

## StopGathering

`StopGathering() -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.StopGathering` — `(...)`

## StopMusic

`StopMusic(fadeOut: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.StopMusic` — `()`

## StopSound

`StopSound(soundHandle: sound, killWhenDone: boolean, fadeOut: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Sound.Stop` — `()`

## StoreInteger

`StoreInteger(cache: gamecache, missionKey: string, key: string, value: integer) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Storage.SetInt` — `(category, key string, v int)`

## StringHash

`StringHash(s: string) -> integer`

- Source: `common.j`
- Maps to: `litd/api.StringHash` — `(s string) int32`

## SuicidePlayer

`SuicidePlayer(id: player, check_full: boolean) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SuicidePlayer` — `(...)`

## SuicidePlayerUnits

`SuicidePlayerUnits(id: player, check_full: boolean) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SuicidePlayerUnits` — `(...)`

## SuicideUnit

`SuicideUnit(count: integer, unitid: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SuicideUnit` — `(...)`

## SuicideUnitEx

`SuicideUnitEx(ct: integer, uid: integer, pid: integer) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.SuicideUnitEx` — `(...)`

## SuspendHeroXP

`SuspendHeroXP(whichHero: unit, flag: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.SuspendExperience` — `(suspend bool)`

## SuspendTimeOfDay

`SuspendTimeOfDay(b: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.SuspendTimeOfDay` — `(suspended bool)`

## TeleportCaptain

`TeleportCaptain(x: real, y: real) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.TeleportCaptain` — `(...)`

## TimerGetElapsed

`TimerGetElapsed(whichTimer: timer) -> real`

- Source: `common.j`
- Maps to: `litd/api.Timer.Elapsed` — `() time.Duration`

## TimerGetRemaining

`TimerGetRemaining(whichTimer: timer) -> real`

- Source: `common.j`
- Maps to: `litd/api.Timer.Remaining` — `() time.Duration`

## TimerGetTimeout

`TimerGetTimeout(whichTimer: timer) -> real`

- Source: `common.j`
- Maps to: `litd/api.Timer.Timeout` — `() time.Duration`

## TimerStart

`TimerStart(whichTimer: timer, timeout: real, periodic: boolean, handlerFunc: code) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.After` — `(d time.Duration, f func()) Timer`

## TownHasHall

`TownHasHall(townid: integer) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.TownHasHall` — `(...) `

## TownHasMine

`TownHasMine(townid: integer) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.TownHasMine` — `(...) `

## TownThreatened

`TownThreatened() -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIView.TownThreatened` — `(...) `

## TownWithMine

`TownWithMine() -> integer`

- Source: `commonai`
- Maps to: `litd/api.AIView.TownWithMine` — `(...) `

## UnitAddAbility

`UnitAddAbility(whichUnit: unit, abilityId: integer) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.AddAbility` — `(ref AbilityRef) Ability`

## UnitAddItem

`UnitAddItem(whichUnit: unit, whichItem: item) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.AddItem` — `(it Item) bool`

## UnitAddItemById

`UnitAddItemById(whichUnit: unit, itemId: integer) -> item`

- Source: `common.j`
- Maps to: `litd/api.Unit.AddItemByType` — `(typ ItemType, opts ...SlotOption) Item`

## UnitCountBuffsEx

`UnitCountBuffsEx(whichUnit: unit, removePositive: boolean, removeNegative: boolean, magic: boolean, physical: boolean, timedLife: boolean, aura: boolean, autoDispel: boolean) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.BuffCount` — `() int`

## UnitDamageTarget

`UnitDamageTarget(whichUnit: unit, target: widget, amount: real, attack: boolean, ranged: boolean, attackType: attacktype, damageType: damagetype, weaponType: weapontype) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.Damage` — `(target Widget, amount float64, opts ...DamageOption) bool`

## UnitDropItemSlot

`UnitDropItemSlot(whichUnit: unit, whichItem: item, slot: integer) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.DropItem` — `(n int) bool`

## UnitDropItemTarget

`UnitDropItemTarget(whichUnit: unit, whichItem: item, target: widget) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.GiveItemTo` — `(n int, to Unit) bool`

## UnitHasBuffBJ

`UnitHasBuffBJ(whichUnit: unit, buffcode: integer) -> boolean`

- Source: `blizzard.j`
- Maps to: `litd/api.Unit.HasBuff` — `(typ BuffType) bool`

## UnitInventoryCount

`UnitInventoryCount(whichUnit: unit) -> integer`

- Source: `blizzard.j`
- Maps to: `litd/api.Unit.ItemCount` — `() int`

## UnitInventorySize

`UnitInventorySize(whichUnit: unit) -> integer`

- Source: `common.j`
- Maps to: `litd/api.Unit.InventorySize` — `() int`

## UnitInvis

`UnitInvis(id: unit) -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AICommander.UnitInvis` — `(...)`

## UnitItemInSlot

`UnitItemInSlot(whichUnit: unit, itemSlot: integer) -> item`

- Source: `common.j`
- Maps to: `litd/api.Unit.ItemInSlot` — `(n int) Item`

## UnitModifySkillPoints

`UnitModifySkillPoints(whichHero: unit, skillPointDelta: integer) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.ModifySkillPoints` — `(delta int) bool`

## UnitRemoveAbility

`UnitRemoveAbility(whichUnit: unit, abilityId: integer) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.RemoveAbility` — `(ref AbilityRef) bool`

## UnitRemoveBuffBJ

`UnitRemoveBuffBJ(buffcode: integer, whichUnit: unit) -> boolean`

- Source: `blizzard.j`
- Maps to: `litd/api.Unit.RemoveBuff` — `(typ BuffType) bool`

## UnitRemoveBuffsEx

`UnitRemoveBuffsEx(whichUnit: unit, removePositive: boolean, removeNegative: boolean, magic: boolean, physical: boolean, timedLife: boolean, aura: boolean, autoDispel: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.RemoveAllBuffs` — `() int`

## UnitShareVision

`UnitShareVision(whichUnit: unit, whichPlayer: player, share: boolean) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Unit.ShareVision` — `(p Player, share bool)`

## UnitUseItem

`UnitUseItem(whichUnit: unit, whichItem: item) -> boolean`

- Source: `common.j`
- Maps to: `litd/api.Unit.UseItem` — `(n int, opts ...UseOption) bool`

## Unsummon

`Unsummon(unitid: unit) -> nothing`

- Source: `commonai`
- Maps to: `litd/api.AICommander.Unsummon` — `(...)`

## VolumeGroupSetVolume

`VolumeGroupSetVolume(vgroup: volumegroup, scale: real) -> nothing`

- Source: `common.j`
- Maps to: `litd/api.Game.SetChannelVolume` — `(ch SoundChannel, v float64)`

## WaitGetEnemyBase

`WaitGetEnemyBase() -> boolean`

- Source: `commonai`
- Maps to: `litd/api.AIThread.WaitGetEnemyBase` — `(...)`
