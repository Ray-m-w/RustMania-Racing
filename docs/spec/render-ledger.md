# Rendering: differences ledger (port/crates/mr-bevy)

What the Bevy presentation does differently from the game's renderer, and why.
Simulation state is never derived from rendering (ADR-0002 split).

| Area | The game | mr-bevy | Why / status |
|---|---|---|---|
| Rasterisation | GPU, affine texture mapping, 15-bit colour, dithering | GPU via wgpu, perspective-correct, 8-bit sRGB, no dither | modern presentation; texel choice, colour modulation (texel*rgb/128) and nearest sampling match |
| Draw order | ordering tables, no depth buffer | depth buffer; layer 1 (far stand-ins) + sky drawn in a first camera pass, layer 0 + kart over it with a fresh depth buffer; layer-0 "ground" list depth-biased under the near list | reproduces the OT layering that matters on DOCK (DAT.md); exact OT order within a layer is not reproduced |
| Near-polygon subdivision (layer 0) | near polygons split 2x2 with interpolated UV/colour | not done | only affects affine warp, unnecessary with perspective-correct mapping |
| Texture LOD (slots 0..3) | UVs >> 1 / >> 2 by depth (decimated copies) | stored UVs always (LOD 0) | LOD 0 is what the game shows up close; distant ground is sharper than the original |
| Per-frame object selection | visibility stream + cull | same rule (Dat::drawn), from the game's camera | exact |
| Animated textures (RAW type 0x22) | frames copied into VRAM over time | static first state | copy routine not identified yet |
| Kart/driver parts | per-part matrices from the kart record | same rules (kartmodel.rs, MDL.md) | exact for the parts; depth-cue colour factors (+0x414..) not applied |
| Lighting / fog | per-vertex colours, depth cue | per-vertex colours only | depth cue not ported |
| Resolution / aspect | 512x240, 4:3 | window size, 4:3 projection (90 x 73.7 deg) | modern; widescreen is a later choice |
| Track objects outside DAT scenery | start/finish banner: cloth from DAT chunk 9 (FUN_800351e8, animated by FUN_80035004); kart shadows (0x800C8B38 list, FUN_8002ADA8); pickups (special slots from chunks 0x0C/0x12/0x13); particles (0x800DF688) | banner drawn with the game's geometry, animation, UVs, shading and 0x2000 cull (scene::ObjectsPlugin, DAT.md "Start/finish banner"); cloth phase for replays taken from the lap1 savestate; shadows, pickups, particles not drawn | banner exact to the GTE vertex; the phase is power-on dependent in the game |
| Kart shadows | one FT4 per kart from the kart record (FUN_8003E91C tail, FUN_8003E59C ground matrix, FUN_8003E3A0), slot 0x14 kart-silhouette tile at kart+0x16C/0x16D, subtractive blend (ABR 2), faded from view z 0x1000 to 0x1800 (world >> 5), +-0x30000 cull (FUN_8002ADA8) | same record (kartshadow.rs, exact on 17/18 oracle records, the other 1 unit off); drawn as a black alpha-blended quad, alpha = 1 - ((0.4 - F)/0.4)^2.2 (B - F matched at a mid-tone, linear-space blend), depth-biased over the ground; interpolated like the kart | subtract approximated (darker grounds are darkened less than the game does); player kart only until Session::others exists |
| HUD | position, lap, timer, minimap, items, banners | position, lap, timer, countdown texts/minigame (hud.md); minimap, item slots, "LAP 2" banner, results not yet | Session 3 minimal HUD |
| AI karts, items, effects (stars, sparks, skid marks, shadow) | drawn | not drawn | AI in Session 3; the rest later |
| Frame rate | 30 Hz logic; 20 Hz in heavy scenes | 30 Hz logic always, rendering interpolated at display rate | intentional: the port never slows down (physics ledger) |
