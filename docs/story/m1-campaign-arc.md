# M1 Campaign Arc -- "Held to the Last" (5 missions)

Section of the Milestone-1 story bible (docs/story/m1-story-bible.md). Doctrine: #1848.
Game spec: #1788 (a 3+ mission hero campaign; persistent hero; cinematics; quests;
triggers). This arc is five missions with a mid-campaign perspective shift.

Structure at a glance:
- Missions 1-3: Emberhold side (heroes Doran Ashheart and Sister Miren).
- Mission 3: the irreversible event -- Sister Miren dies, permanently, and the player
  cannot prevent or undo it (doctrine rule 3; our emotional edge).
- Mission 4: perspective shift to the Veilspun side (Yvenne Duskblade and Caul the
  Whisperer) -- the recognition/reversal: the player learns the enemy of Missions 1-3
  had their own grief, and that Miren's death came from a fog-of-war misrecognition, not
  villainy.
- Mission 5: convergence -- a hard peace that resolves the war WITHOUT un-writing the
  loss.

Per-mission beat map uses four beats: SETUP/PROMISE, ESCALATION, THE IRREVERSIBLE (the
turn that cannot be taken back), AFTERMATH. Runtime systems are cited by issue so mission
authoring exercises real engine surfaces: economy #1785, AI #1786, triggers #803,
cinematics #892 (timeline) / #845 (cutscene), quests #839, save/load #850, fog #847,
minimap #816, victory/defeat #838.

================================================================================
## Mission 1 -- "The Watch That Held" (Emberhold; teaches the player and the wall)

Premise: Doran holds a forward Emberhold outpost above Ashford Crossing as Veilspun
raids probe the dark. Tutorial-weight, warm in tone, so the fall later lands harder.

Beat map:
- Setup/promise: Doran, competent and calm, welcomes the player to the Compact; the
  promise is "we build, we hold, no one is lost on my watch." Establish Doran's
  competence hook (Bulwark Stance saves the first probe).
- Escalation: raids grow; the player must build economy and a small wall to survive
  timed night-attacks.
- The irreversible (small, teaching): a straggler patrol is caught in the open and the
  player will lose one or two named troopers if slow -- a first, low-stakes taste that in
  this world the dead stay dead (no reload-erases-it framing in canon).
- Aftermath: Miren arrives to mend the wounded; her competence and go-back-for-stragglers
  loyalty are introduced. Doran and Miren's warmth is established -- the bond the player
  will grieve.

Objectives sketch: build a keep and granary; train a defensive core; survive three night
raids; keep Doran alive.

Runtime systems exercised: economy #1785 (gold/lumber/food gather and spend); basic AI
#1786 (scripted raid waves); triggers #803 (timed night waves, straggler event); quests
#839 (build / survive objectives); fog #847, minimap #816; victory #838; save/load #850
baseline (hero Doran persists to M2 with XP/items).

Cinematic beats (#892/#845): cold-open cutscene of Doran on the night wall reciting the
Compact oath, unable to finish naming his Guttering dead (vulnerability hook planted).
Mission-end interlude: Miren jokes that he builds walls so he never has to say goodbye.

================================================================================
## Mission 2 -- "Stone Answers Shadow" (Emberhold; escalation and the tech tier)

Premise: the Compact pushes to fortify Ashford proper. Veilspun resistance sharpens; the
player unlocks tier-2 tech and heroes gain levels.

Beat map:
- Setup/promise: a clear, achievable goal -- wall the Crossing at last, honor Corramere.
- Escalation: Veilspun shadeweaving hides their strength; fog and ambush punish the slow;
  the player must combine Doran's front line and Miren's sustain to break through.
- The irreversible: the Compact commits to the Crossing publicly -- an oath is cut in
  stone that cannot be unsworn (the Unturning made civic). From here neither side can
  concede without a second failure.
- Aftermath: a captured Veilspun scout dies rather than reveal what the Court is hiding;
  first hint that the enemy is protecting something, not just attacking (seeds Mission 4's
  recognition).

Objectives sketch: reach tier-2 (Citadel/Forge Mastery/War Doctrine); destroy the
Veilspun forward Loom; raise the first Ashford ward-stone; keep both heroes alive.

Runtime systems: economy #1785 (larger, upkeep pressure); AI #1786 (aggressive ambush
profile using fog); triggers #803 (oath-cutting scripted beat, prisoner scene); quests
#839 (tech + destroy + build); cinematics #892/#845 (oath cutscene); save/load #850
(persist Doran and Miren, levels, items into M3 -- the persistence is what will make the
M3 loss hurt).

Cinematic beats: the oath-cutting -- Doran's hand on the ward-stone, Miren's warding light
consecrating it. A quiet two-hander: Miren confesses her old failure (the ward she lowered
too early) and her fear of failing to save someone. Dramatic irony is now loaded.

================================================================================
## Mission 3 -- "The Ward at Ashford" (Emberhold; THE IRREVERSIBLE LOSS)

Premise: a column of Emberhold refugees -- elders and children displaced by the fighting
-- must cross Ashford under fire to reach the walled interior. Miren raises a Radiant
Bastion over them. The player defends the crossing.

Beat map:
- Setup/promise: a rescue -- get the column across; the game frames it as winnable and the
  player expects to save everyone (the promise is deliberately generous to make the break
  land). Miren's go-back-for-stragglers loyalty is at maximum; she will not lower the ward.
- Escalation: a fog-shrouded Veilspun assault (unusually fierce -- because, unknown to the
  player, the attackers believe this is an Emberhold war-march, not refugees) hammers the
  crossing. The player holds, and mostly succeeds -- the column is getting across.
- The irreversible: the assault breaks through to Miren's position. By the oath-logic of
  wardsmithing, Miren cannot lower the Radiant Bastion while refugees still stand within
  it, and the last of them are still crossing. The mission is engineered so the ONLY way
  the column survives is for Miren to hold the ward to the last -- and she does. She is
  killed holding it. The player is given every tool and cannot save her: there is no
  positioning, no ability, no rush that both saves the column and saves Miren; the design
  forecloses the take-back the way the Unturning forecloses it. She cannot be revived (she
  spent all of herself; spent cannot be un-spent). The refugees live because she is gone.
  This is doctrine rule 3 as the emotional core of the campaign.
- Aftermath: silence at the crossing. Doran reaches her too late. The unbreakable man
  cannot fortify against this; for the first time the wall did not hold what mattered.

Objectives sketch: escort the refugee column across Ashford; hold the crossing; (hidden,
un-preventable) Sister Miren falls holding the ward. Victory condition is the column's
survival -- a victory that costs the loss, so the player wins and grieves at once.

Runtime systems: cinematics #892/#845 (heavy -- the ward-fall is a scripted timeline
sequence, not a gameplay death that a reload erases in canon); triggers #803 (the
un-winnable-to-save-Miren scripted event; a quest failstate that is NOT game-over -- the
mission continues and is won even though Miren is lost, teaching that loss and failure are
different, per the permadeath-grief research); quests #839 (escort with a partial-success
model); economy/defense #1785; AI #1786 (fog assault profile); fog #847; save/load #850
(Doran persists to M5 WITHOUT Miren -- her absence is carried forward in the save state,
making irreversibility literally a property of the file).

Cinematic beats: the hold and the fall, intercut with the last refugee child reaching the
gate. Closing shot: Doran setting Miren's chaplain-token into the ward-stone -- the first
time he builds a memorial rather than a wall. He still cannot say the names; now hers is
among them.

================================================================================
## Mission 4 -- "What the Fog Kept" (Veilspun; the perspective shift and recognition)

Premise: the campaign hands the player Yvenne and Caul and rewinds nothing -- it moves
FORWARD to the same events seen from the Court's side, and past them. The player learns
who they were fighting and why, and that Miren's death came from Yvenne's fog-blind
ambush on what she believed was a war-march.

Beat map:
- Setup/promise: play the Court sympathetically -- guard the hidden remnant, keep the
  children unfound; the promise is "protect what is left." Yvenne's speed and the
  remnant's fragility are the hooks.
- Escalation: Caul's intelligence reveals the Emberhold are fortifying Ashford, which will
  in time expose the remnant; the Court must respond. Yvenne leads a fog-shrouded strike on
  an Emberhold column reported as a war-march.
- The irreversible (recognition/reversal): in the clearing fog Yvenne sees what she struck
  -- refugees, a fallen chaplain, a ward-stone. She, who rages against irreversible loss,
  has caused one she can never take back. This is the anagnorisis: the two "sides" were the
  same grief, and the catastrophe was error, not evil. Nothing can undo it.
- Aftermath: Caul, keeper of names, must add Miren -- an enemy -- to the Court's
  memory-cloth, because a death is a death and none is un-remembered. The seed of peace:
  both peoples now share a loss and a secret (the buried heart under Ashford).

Objectives sketch: escort the remnant's children to a new refuge; scout the Emberhold
fortification; conduct the fog strike; (scripted) discover its true cost.

Runtime systems: economy #1785 (Veilspun asymmetric economy -- faster, thinner); AI #1786
(Emberhold now the AI opponent, using the tanky-defensive profile the player fought AS in
M1-3 -- role reversal reinforces recognition); stealth/fog #847 (Veilspun concealment is
mechanically central here); triggers #803 (the fog-clear recognition beat); quests #839
(escort + scout + strike); cinematics #892/#845 (the recognition cutscene); save/load #850
(Yvenne and Caul persist to M5).

Cinematic beats: the fog strike from inside Yvenne's certainty, then the fog lifting on the
truth. Caul weaving Miren's name into the cloth by candlelight, telling Yvenne the oldest
Court truth: you cannot outrun the Unturning; you can only choose what you weave next.

================================================================================
## Mission 5 -- "A Gate in the Wall" (both sides; the finale that keeps the loss)

Premise: the war cannot be won by either side without destroying the buried heart both
depend on. Doran (who has learned to grieve) and Caul (who must reveal a kept truth) broker
a hard peace at Ashford; Yvenne must accept the death she caused; a last threat forces the
former enemies to hold the Crossing together.

Beat map:
- Setup/promise: a fragile parley -- the promise is not victory but survival together.
- Escalation: hardliners on both sides (and the danger of the destabilized buried heart
  itself after all the fighting) threaten to collapse the Crossing and the refuges beneath.
  The player must fight a combined-arms defense using both factions' heroes and units.
- The irreversible: Caul reveals the kept truth into the open -- both peoples draw life
  from the same buried heart, the Court's remnant and the Compact's dead are one city
  still. Once said, it cannot be unsaid (the Unturning cuts both ways -- a truth spoken is
  also permanent). It changes everything and dooms the old war for good.
- Aftermath / resolution: the two peoples do not merge and do not forgive cheaply; they
  build a shared ward at Ashford -- a wall with a gate -- co-guarding the heart. Miren stays
  dead. The peace is built ON the loss, not over it: the Emberhold set her token in the
  ward-stone; the Veilspun weave her name in the memory-cloth; the gate is named for her.
  Doran finally says the names aloud, hers among them. Yvenne stands still. Caul lets the
  light in. Nothing is undone; the world goes forward carrying the loss -- which is the only
  kind of ending the Unturning allows, and the doctrine-faithful one.

Objectives sketch: hold the parley; defend the Crossing with combined heroes/units;
stabilize the buried heart; complete the shared ward. Victory #838 = the Crossing stands
and both peoples live -- with the loss intact.

Runtime systems: economy #1785 (both factions available); AI #1786 (hardliner opponents
from both sides); triggers #803 (parley, the reveal, the stabilization); quests #839
(defend + build + parley); cinematics #892/#845 (the reveal and the naming of the gate);
save/load #850 (final state loads Doran, Yvenne, Caul persisted; Miren absent -- the save
file itself is the proof that nothing was taken back).

Cinematic beats: the reveal in the open; the building of the gated ward; the naming; the
last shot -- a gate standing open in a wall, shadow and stone keeping one crossing together,
one name cut above the arch.

================================================================================
## Why this arc lands (mapped to the research)

- One identifiable loss, not a statistic: a single named, competent, vulnerable, loyal
  character the player controlled and depended on -- maximizing the identifiable-victim /
  singularity effect and parasocial-attachment grief.
- Inevitability, not shock: the fall is foreshadowed (Miren's confessed fear, the loaded
  oath-logic) and then made un-preventable -- Aristotle's tragedy from inevitability and
  error (hamartia), heightened by the certainty effect and loss aversion (a certain,
  foreseeable loss the player is powerless to trade away).
- Permadeath as meaning, not punishment: the loss is a scripted canonical death with no
  take-back, which the permadeath studies tie to grief and, through grief, to appreciation
  and meaningfulness -- exactly the doctrine's "emotional edge."
- Recognition and mirrored antagonists: the Mission 4 reversal makes the enemy the same
  grief, satisfying the want/need craft of mirrored protagonist/antagonist and the tragic
  geometry of doomed loyalty and irreversible error.
- Perspective shift and hero persistence: the mid-campaign POV change and the heroes who
  persist (and one who is permanently absent) follow the RTS-campaign craft that made
  hero-driven campaigns land -- and turn the save file into a record of the Unturning.
