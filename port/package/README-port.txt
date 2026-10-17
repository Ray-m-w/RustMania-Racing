RustMania Racing for Windows x64, version @VERSION@
===================================================

An unofficial fan reimplementation of Muppet RaceMania (PlayStation, 2000, SLUS-01237) in
Rust. It contains NO game data: it reads everything (models, textures,
music, movies, the game's tables) from YOUR OWN copy of the disc at run
time. You need a dump of the USA disc, "Muppet RaceMania (USA)".


Requirements: Windows 10/11 x64 and a GPU with Vulkan or Direct3D 12.
Nothing else to install (the program carries its C runtime).
Controls: keyboard or gamepad (XInput / DirectInput pads; rumble).


1. Point it at your disc
------------------------
Start rustmania-racing.exe. Without a disc it asks for it in its window:
type the path to your disc image, or paste it with Ctrl+V (in Explorer:
Shift + right-click the file -> "Copy as path"), and press Enter, e.g.

    D:\Games\Muppet RaceMania (USA).cue

A .cue found next to the program or in the data folder (see 3.) is listed
with a number: press that number. The path is checked, saved in
settings.txt as `disc = ...` and the game starts; Esc quits. To change the
disc later, edit (or delete) the `disc` line in settings.txt.

The .cue (single FILE, MODE2/2352, as Redump dumps are) or its track .bin
both work. A .chd must be extracted first:
    chdman extractcd -i "Muppet RaceMania (USA).chd" -o "Muppet RaceMania (USA).cue"
Other ways to name the disc: `rustmania-racing.exe --disc PATH`, or the
environment variable MRM_DISC.

On the first start with a disc the track file is checked once against the
Redump record (disc 16664, track 1, SHA-1
1b0fc0b6fa0958ecac5f087aab4b444b2067a387). A different image still runs,
with a warning in the window title; the result is kept in
disc-check.txt (delete it to check again).


2. Controls
-----------
Keyboard, player 1:
    arrows      steer / menu up, down, left, right
    X           accelerate (CROSS; menus: select)
    S           brake / reverse (CIRCLE)
    Z           jump (SQUARE)
    A           camera lock-on while held (TRIANGLE; menus: back)
    Enter       START (pause)
    Q / W       L1 / R1 (turbo; special move on a full meter)
    E / R       L2 / R2 (character special)
Gamepad: A/Cross accelerate (or right trigger), B/Circle brake (or left
trigger), X/Square jump, Y/Triangle lock-on, bumpers L1 / R1, stick clicks
L2 / R2, Start pause, D-pad or left stick steer. A second pad plays player 2.
OPTIONS -> SET CONTROLLERS swaps the game's layout A / B.

Two players (2 PLAYER RACE / BATTLE / STUNT RACE, CHICKEN CHASE on a
course's menu): split screen, player 1 on top, two pads or the keyboard;
player 2:
    J / L steer, I / K up / down, N accelerate (CROSS), B brake (CIRCLE),
    M jump (SQUARE), H (TRIANGLE), Backspace = START, U / O = L1 / R1,
    Y / P = L2 / R2.
Both players pick their characters at the same time (CROSS = ready,
TRIANGLE = not ready). COURSE SECRETS rows switch between 1 PLAYER and
2 PLAYERS with LEFT / RIGHT; the main menu's TOURNAMENT switches to
2 PLAYERS the same way (a three-kart league for two).


3. Your files (the data folder)
-------------------------------
    %APPDATA%\MuppetRaceMania\
        settings.txt    options, one `key = value` per line (every key is
                        listed with its help; OPTIONS -> PORT SETTINGS in the
                        game writes it too)
        save.bin        your progress (written automatically; OPTIONS ->
                        MEMORY CARD saves / loads it on demand)
        crash.txt       written if the program stops on an error: send it
                        with a bug report
        disc-check.txt  the disc check above

Paste %APPDATA%\MuppetRaceMania into the Explorer address bar to open it.
Other places:
    --data-dir PATH     use that folder instead
    portable.txt        an (empty) file named portable.txt next to the exe
                        keeps all of the above next to the exe
A PS1 memory card image with a Muppet RaceMania save can be imported once
(read only) from saves\card1.mcd in the data folder, or with
--import-card PATH, when there is no save.bin yet.

Useful settings (settings.txt): fullscreen = true, resolution = 1920x1080,
aspect = native | 4:3, faithful_view = true (the PlayStation's look),
logic_hz = 30 | 60, skip_intro_movies = true, music_volume / sound_volume
(0..15). Any setting can be given on the command line too:
--set key=value.


4. Known differences
--------------------
Every mode and menu of the original is ported. By design: the default
60 Hz game logic is tuned to feel like the original's 30 Hz (logic_hz =
30 is the exact original timing); enhancements (widescreen, smoother
textures and lighting, full draw distance, anti-aliasing) are options
(faithful_view = true for the PlayStation's look). OPTIONS -> ADJUST
SCREEN moves the picture inside the window's black border (it cannot move
when the picture fills the window). VIBRATION rumbles the gamepad.


5. Licences
-----------
THIRD-PARTY-LICENSES.txt lists the open-source Rust crates compiled into
the program and their licences. Muppet RaceMania and its content belong
to their owners; this package contains none of it.
