# Diffusion / Neural World Rendering for Games -- State of the Art, July 2026

**Research date: 2026-07-12. Web research (Exa + web search).**

**Frame of reference:** our min-spec is a $500 Windows PC -- GTX 1650 class or an iGPU, ~4GB
VRAM, no tensor cores, 8-16GB RAM, Rust + wgpu engine. Every verdict below is against that
box.

---

## 1. Interactive diffusion/neural world models -- what exists and where it runs

The one-sentence summary: **every single interactive world model in 2025-2026 runs on
datacenter hardware (H100s or TPU pods) and streams video to the client. None run on consumer
GPUs at playable quality, and none are within two orders of magnitude of a GTX 1650.**

| System | What it does | Perf | Where it runs |
|---|---|---|---|
| **Google Genie 2** (Dec 2024) | Text/image -> interactive 3D-ish world | 360p, ~15fps, coherent 10-20s | Google TPUs |
| **Google Genie 3** (Aug 2025) | Text -> photorealistic explorable world, promptable events | 720p 24fps, ~50ms control latency, coherent ~1 min, "largely" consistent for several minutes | Google TPU v5 infrastructure; reports cite >= 8 TPU v5 chips per session for real-time inference. 11B-param autoregressive transformer. (https://deepmind.google/blog/genie-3-a-new-frontier-for-world-models/, https://www.uploadvr.com/google-genie-3-ai-world-models-the-path-to-photorealistic-vr/) |
| **Project Genie** (Jan 2026) | Consumer-facing Genie 3 prototype for Google AI Ultra subscribers (US) | Same class | Cloud-streamed only (https://blog.google/innovation-and-ai/models-and-research/google-deepmind/project-genie/) |
| **Decart Oasis 1** (Nov 2024) | "AI Minecraft" -- first playable diffusion world | 720p ~20fps, drifts in seconds | **One H100 per player** (https://www.tweaktown.com/news/101466/) |
| **Decart Mirage** (Jul 2025) & **Oasis 2.0** (Sep 2025) | Real-time video *restyling* -- a Fabric mod streams your actual Minecraft frames to Decart, which re-skins them live in any style. Note: Oasis 2 is restyling a real engine's output, not generating the world | Real-time, "infinite" duration | Decart cloud via WebRTC (https://github.com/DecartAI/mirage-minecraft-mod) |
| **Decart Oasis 3** (Jun 2026) | Photorealistic driving world model, actions-in/frames-out, aimed at AV/robotics RL | 768px, ~22fps, <200ms latency, unlimited duration; ~8,000 tokens per generated frame | Cloud API only ($0.02/sec ~= **$72/hour per user**), gRPC (https://techcrunch.com/2026/06/10/decarts-new-world-model-can-simulate-hours-of-photorealistic-driving-with-some-caveats/, https://docs.platform.decart.ai/models/realtime/oasis-3) |
| **DIAMOND** (NeurIPS 2024) | Open-source diffusion world model; playable CS:GO and Atari dreams | CS:GO at **~10fps, low res, on an RTX 3090** -- the only one that runs on a consumer GPU at all, and it's a blurry curiosity | Local, https://github.com/eloialonso/diamond |
| **Microsoft Muse / WHAM** (Feb 2025) + **WHAMM** Quake II demo (Apr 2025) | Gameplay-ideation model trained on Bleeding Edge; Quake II demo at ~10fps, 640x360, seconds of memory | Toy resolution/fps | Inference tested on RTX A6000 / A100; trained on H100s. Tom's Hardware called it "incredibly impractical" for actual dev (https://www.microsoft.com/en-us/research/blog/introducing-muse-our-first-generative-ai-model-designed-for-gameplay-ideation/, https://www.tomshardware.com/video-games/xbox/microsoft-xbox-showcases-an-incredibly-impractical-ai-model-for-gameplay-ideation) |
| **Odyssey-1 / Odyssey-2 / -2 Max** (2025) | "Interactive video" -- frame every 40ms, up to 30fps | Streams from **H100 clusters at $1-2 per user-hour**; raised $310M to keep scaling | Cloud only (https://techcrunch.com/2025/05/28/odysseys-new-ai-model-streams-3d-interactive-worlds/, https://odyssey.ml/) |
| **Runway GWM-1** (late 2025) | General world model, game-world demos | Research preview | Cloud |
| **Tencent Hunyuan-GameCraft 1.0** (Aug 2025) / **GameCraft-2** (Nov 2025, https://arxiv.org/pdf/2511.23429) | Open-weights image+actions -> game video; distilled variant is "interactive" at ~6-7fps | Reference inference uses multi-GPU; quantized 13B fits an RTX 4090 (24GB) -- still 6x our VRAM budget (https://github.com/Tencent-Hunyuan/Hunyuan-GameCraft-1.0) |
| **Skywork Matrix-Game 2.0** (Aug 2025) | First fully open-source *real-time* interactive world model; 25fps, minute-scale coherence, keyboard/mouse injection | **25fps on a single H100; requires >= 24GB NVIDIA GPU** (https://arxiv.org/html/2508.13009v1, https://github.com/SkyworkAI/Matrix-Game/tree/main/Matrix-Game-2) |

**Verdict vs. our min-spec: not shippable, not close.** The cheapest "playable" open model
needs a 24GB datacenter/enthusiast GPU per player; the good ones need H100s or TPU pods per
session. A GTX 1650 has ~4GB VRAM and roughly 1/50th the effective throughput of an H100 --
and no tensor cores. Cloud streaming is the only delivery path, and Odyssey's own numbers
($1-2/user-hour) and Decart's pricing ($72/user-hour for Oasis 3) make it economically absurd
for a $30 indie RPG. Also every one of these still has world-coherence horizons measured in
minutes and no persistent game state, inventory, quest logic, or determinism.

---

## 2. Neural rendering that could actually help a shipped game on low-end hardware

This is where real, shippable value exists -- but tensor-core gating is the recurring trap.

**Neural Texture Compression (NVIDIA RTXNTC + DirectX Cooperative Vectors)** -- the most
relevant to us.

- Compresses texture sets 8-20x beyond BC7; demos show VRAM for a texture set dropping from
  6.5GB to ~1GB
  (https://www.tomshardware.com/pc-components/gpus/nvidia-ai-tech-claims-to-slash-vram-usage-by-85-percent-with-zero-quality-loss-neural-texture-compression-demo-reveals-stunning-visual-parity-between-6-5gb-of-memory-and-970mb,
  https://www.tomshardware.com/pc-components/gpus/benchmarking-nvidias-rtx-neural-texture-compression-tech-that-can-reduce-vram-usage-by-over-80-percent).
- **Crucially, it has non-tensor fallbacks.** Per the RTXNTC SDK
  (https://github.com/NVIDIA-RTX/RTXNTC): "inference on load" (decompress NTC -> BC at load
  time) needs only Shader Model 6 and is **validated on GTX 1000-series, Radeon RX 6000, and
  Intel Arc**; "inference on sample" (decode in the pixel shader) works via DP4a/integer-math
  fallback but is 2-4x slower than the Cooperative Vector path on Ada/Blackwell. Cooperative
  Vectors exist in both DX12 (Agility SDK preview) and **Vulkan** -- relevant since wgpu sits
  on Vulkan/DX12; native extension access (wgpu-hal) would be needed since wgpu doesn't
  expose coop-vectors yet.
- **Fit for us: "inference on load" mode is genuinely viable on a GTX 1650** -- ship NTC files
  (smaller downloads/disk), transcode to BCn at load. On-sample inference on min-spec: no.
- **Verdict: the one neural-rendering tech worth prototyping in 2026.** Medium integration
  effort; SDK is source-available under NVIDIA RTX SDK license (check redistribution terms).

**Learned upscaling.**

- **DLSS**: requires RTX tensor cores -- locked out on GTX 1650. Dead for min-spec.
- **FSR 4**: now requires RDNA4 AI accelerators. Dead for min-spec. **FSR 2/3.1** remain
  pure-shader, run anywhere, and are open source (MIT) -- the practical answer for a wgpu
  engine.
- **Intel XeSS (DP4a path)**: the only *AI* upscaler that runs on non-tensor hardware -- DP4a
  fallback covers GTX 10-series+, RX 6000+. Caveats: GTX 1650 (Turing TU117) does support
  DP4a; quality/gain are lower on the fallback path; XeSS ships as a closed DLL against
  DX12/Vulkan
  (https://www.windowscentral.com/gaming/what-is-super-resolution-nvidia-dlss-amd-fsr-intel-xess-and-microsoft-directsr-explained).
- **Verdict: ship FSR 2/3-class shader upscaling (or a hand-rolled TAAU) as the baseline;
  consider XeSS-DP4a as an optional quality tier.** Mature, proven, and integration into wgpu
  is very doable.

**Neural Radiance Cache (RTXGI 2.0 NRC)**: trains a tiny MLP per-frame to cache path-traced
radiance. **Hard-requires RTX tensor cores + ray tracing** (https://github.com/NVIDIA-RTX/RTXGI).
Its sibling **SHaRC (Spatial Hash Radiance Cache)** is vendor-neutral and non-neural -- but
both presuppose a path tracer, which the min-spec can't run anyway. **Verdict: irrelevant to
our game; classic baked/probe GI is still the right answer for stylized RPG on iGPU.**

**Neural materials/shaders (NVIDIA "RTX Neural Shaders", neural BRDF approximation)**:
research/SDK-preview stage, cooperative-vector dependent, no shipped titles yet. Watch, don't
build.

---

## 3. Generative AI as a dev-time asset pipeline (this is where the actual leverage is)

Runs on the dev machine or cloud, ships as ordinary meshes/textures -- zero runtime cost, zero
min-spec impact.

**Text/image-to-3D:**

- **Meshy** -- cloud, browser, text/image -> PBR-textured mesh, auto-rigging, 500+ animation
  presets, quad remesh. **Paid plans include full commercial rights.** Most
  game-pipeline-complete option; quality for stylized/low-poly RPG props is genuinely usable
  in 2026 (hero characters still need artist cleanup). (https://www.meshy.ai/compare)
- **Tripo 2.5/3.x** -- similar, fastest generation (~10s), good stylized output, commercial
  terms on paid tiers (https://www.tripo3d.ai/compare/tripo-vs-hunyuan).
- **Microsoft TRELLIS / TRELLIS 2** -- open weights, best free image-to-3D; needs a 24GB Linux
  GPU to self-host. **Licensing trap: core is MIT but the texture pipeline depends on
  nvdiffrast (NVIDIA Source Code License -- non-commercial without NVIDIA authorization)**
  (https://trellis2.app/blog/best-ai-3d-model-generator).
- **Tencent Hunyuan3D 2.1/3.0** -- strong open-weights shape+texture; self-host needs 10-29GB
  VRAM. **Licensing trap: Tencent community license excludes EU/UK/South Korea for the OSS
  weights** -- a real problem if selling there. Static meshes only, no rigging.
- Maturity overall: props, environment dressing, weapons, furniture = production-usable with
  retopo/cleanup pass. Topology is still auto-generated (fine for static, mediocre for
  deforming characters).

**Text-to-texture/material:** mature and safe -- Stable Diffusion/Flux + ComfyUI locally (runs
fine on a dev 4090/5090), Meshy's texture mode, Substance 3D Sampler's image-to-material.
Great fit for stylized RPG trim sheets, terrain albedos.

**Image-to-3D world scaffolding -- World Labs Marble** (launched commercially Nov 2025):
text/image/video/panorama -> persistent, downloadable 3D scene. **Yes, it exports Gaussian
splats** (SPZ/PLY, ~2M or 500k splats) plus collider meshes on the $20/mo Standard tier;
**textured GLB mesh + commercial rights need the $35/mo Pro tier**
(https://docs.worldlabs.ai/marble/export/gaussian-splat,
https://docs.worldlabs.ai/marble/export/specs,
https://techcrunch.com/2025/11/12/fei-fei-lis-world-labs-speeds-up-the-world-model-race-with-marble-its-first-commercial-product/).
Caveats: worlds are walkable only within a bounded region; splat rendering in a custom wgpu
engine means writing/porting a splat rasterizer (doable -- reference implementations exist,
and 500k-splat scenes can render on modest GPUs -- but it's a distinct render path with its
own perf/lighting/collision problems). The GLB mesh export is the lower-friction route: treat
Marble as a **concept-to-blockout tool**, generate a scene, export mesh, retopo/stylize in
Blender.

- **Odyssey/Genie do NOT give exportable assets** -- Marble is uniquely useful because output
  is persistent 3D, not a video stream.

**LLM-assisted procedural generation:** the quietly most practical technique -- use LLMs at
dev time (or offline batch) to author quest data, dialogue trees, item stats, area
descriptions, and to write/tune procedural generators. Zero runtime cost, fully deterministic
shipped output. The repo's existing content-pipeline approach (spec-authored areas ingested
as data) is exactly the shape that benefits.

**Verdict: this is the layer to invest in now.** Recommended stack for a stylized RPG on our
budget: Meshy or Tripo paid tier (clean commercial rights) for props -> Blender cleanup;
SD/Flux for textures; Marble Pro experimentally for environment blockouts; LLMs for content
data. Avoid self-hosted Hunyuan3D/TRELLIS for shipped assets until the licensing issues are
resolved.

---

## 4. Honest feasibility: realtime diffusion worlds on a $500 PC, 2026-2028?

**No. Not in that window, and it's not particularly close.**

The arithmetic: Matrix-Game 2.0 -- the *most efficient open* real-time world model -- needs a
full H100 (~1000 INT8 dense TOPS, 80GB HBM) for 25fps at modest resolution. A GTX 1650
delivers ~3 TFLOPS FP32, ~4GB VRAM, no tensor cores, no FP8/INT8 acceleration worth naming.
That's a ~100-300x compute gap plus a 20x memory gap, against models whose *quality* is still
below a 2010 game engine in coherence, resolution, and interaction fidelity. Distillation and
4-bit tricks are yielding maybe 5-10x per generation -- say the gap closes to "runs on a 5090"
by 2028 (plausible; Decart's own optimization stack and TechCrunch's coverage point that
way). Min-spec $500 hardware also barely improves: it has no NPU/tensor path at all. Decart's
CEO himself flags the fundamental architecture cost: ~8,000 tokens *per frame*,
autoregressive, context filling in seconds
(https://techcrunch.com/2026/06/10/decarts-new-world-model-can-simulate-hours-of-photorealistic-driving-with-some-caveats/).

Beyond compute, the unsolved product problems: no persistent game state, no determinism
(saves, multiplayer, replays -- our engine has a replay/telemetry system that would be
impossible), coherence horizons of minutes, no authored-content control, and legibility/UI
generation failures (DeepMind lists these as "ongoing research challenges").

What informed people actually say:

- **John Carmack & Tim Sweeney** (on the WHAMM Quake II demo, Apr 2025): both *defended the
  research* against job-loss outrage -- Carmack called it "impressive" and analogized to
  engines replacing hand-coded rendering -- but framed it strictly as a **tool trajectory**,
  with Carmack noting the best games will still be built by dedicated teams; neither claimed
  neural frame generation replaces engines on any near horizon
  (https://www.techspot.com/news/107483-john-carmack-tim-sweeney-defend-microsoft-whamm-ai.html).
- Working game devs' consensus (visible in the Muse backlash,
  https://www.ghacks.net/2025/02/20/microsofts-muse-ai-model-faces-backlash-from-game-development-community/,
  https://www.tomshardware.com/video-games/xbox/microsoft-xbox-showcases-an-incredibly-impractical-ai-model-for-gameplay-ideation):
  compute cost per user-hour is wildly uneconomical vs. a traditional engine that renders a
  better result on a $150 GPU, and the tech's real near-term customers are **robotics/AV
  simulation** (note Oasis 3 and Genie 3 both pivoted their pitch exactly there).
- The labs' own positioning confirms it: DeepMind pitches Genie 3 at *agent training*; Decart
  pitches Oasis 3 at *autonomous vehicles*; World Labs ships *exportable assets* rather than
  runtime streaming. The industry has already voted on where this is useful in the 2020s:
  **dev-time and simulation, not client runtime.**

### Bottom line for our engine

1. **Runtime diffusion world rendering: verdict -- no.** Wrong hardware class by 100x+, wrong
   economics, wrong fit for a deterministic RPG. Re-evaluate ~2028, and even then expect
   "RTX 6090 feature," never "$500 PC feature."
2. **Neural rendering on min-spec: verdict -- two things only.** (a) NTC "inference on load"
   for texture disk/VRAM footprint (works on SM6-class GPUs incl. GTX 1650, validated by
   NVIDIA on GTX 10-series); (b) FSR2/3-class shader upscaling, optionally XeSS-DP4a. Skip
   DLSS-dependent, NRC, and neural-shader paths -- all tensor-gated.
3. **Asset pipeline: verdict -- adopt now.** Meshy/Tripo (paid, commercial rights) for props,
   SD/Flux for textures, Marble Pro for environment scaffolds (splat or GLB export), LLMs for
   content data. This is mature, cheap, license-clean if you pick the hosted tools, and
   touches nothing on the runtime.

### Key sources

- https://deepmind.google/blog/genie-3-a-new-frontier-for-world-models/
- https://blog.google/innovation-and-ai/models-and-research/google-deepmind/project-genie/
- https://techcrunch.com/2026/06/10/decarts-new-world-model-can-simulate-hours-of-photorealistic-driving-with-some-caveats/
- https://docs.platform.decart.ai/models/realtime/oasis-3
- https://arxiv.org/html/2508.13009v1 (Matrix-Game 2.0)
- https://github.com/NVIDIA-RTX/RTXNTC ; https://github.com/NVIDIA-RTX/RTXGI
- https://www.worldlabs.ai/blog/marble-world-model ; https://docs.worldlabs.ai/marble/export/specs
- https://techcrunch.com/2025/05/28/odysseys-new-ai-model-streams-3d-interactive-worlds/
- https://www.microsoft.com/en-us/research/blog/introducing-muse-our-first-generative-ai-model-designed-for-gameplay-ideation/
- https://www.techspot.com/news/107483-john-carmack-tim-sweeney-defend-microsoft-whamm-ai.html
- https://github.com/eloialonso/diamond ; https://github.com/Tencent-Hunyuan/Hunyuan-GameCraft-1.0
- https://trellis2.app/blog/best-ai-3d-model-generator
- https://www.tomshardware.com/pc-components/gpus/benchmarking-nvidias-rtx-neural-texture-compression-tech-that-can-reduce-vram-usage-by-over-80-percent
