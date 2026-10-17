Muppet RaceMania (Rust + Bevy port) for Windows x64, version @VERSION@
=====================================================================

A reimplementation of Muppet RaceMania (PlayStation, 2000, SLUS-01237) in
Rust. It contains NO game data: it reads everything (models, textures,
music, movies, the game's tables) from YOUR OWN copy of the disc at run
time. You need a dump of the USA disc, "Muppet RaceMania (USA)".


Requirements: Windows 10/11 x64, a GPU with Vulkan or Direct3D 12, and
the Microsoft Visual C++ 2015-2022 Redistributable (x64; VCRUNTIME140.dll,
already present on most PCs).


1. Point it at your disc
------------------------
Start muppet-racemania.exe once. Without a disc it shows a message and
creates the settings file (see 3. below for where). Open settings.txt in a
text editor and set the `disc` line to your disc image:

    disc = D:\Games\Muppet RaceMania (USA).cue

The .cue (single FILE, MODE2/2352, as Redump dumps are) or its track .bin
both work. A .chd must be extracted first:
    chdman extractcd -i "Muppet RaceMania (USA).chd" -o "Muppet RaceMania (USA).cue"
Other ways to name the disc: `muppet-racemania.exe --disc PATH`, or the
environment variable MRM_DISC.

On the first start with a disc the track file is checked once against the
Redump record (disc 16664, track 1, SHA-1
1b0fc0b6fa0958ecac5f087aab4b444b2067a387). A different image still runs,
with a warning in the console and the window title; the result is kept in
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
Gamepad (not read by this build, see 4.): D-pad / left stick steer, south or right trigger accelerate,
east or left trigger brake, west jump, north = TRIANGLE, Start = START,
bumpers = L1 / R1, stick clicks = L2 / R2.
OPTIONS -> SET CONTROLLERS swaps the game's layout A / B.

Two players (2 PLAYER RACE / BATTLE / STUNT RACE, CHICKEN CHASE on a
course's menu): split screen, player 1 on top. With two gamepads the first
plays player 1, the second player 2. With one gamepad or none, player 2 on
the keyboard:
    J / L steer, I / K up / down, N accelerate (CROSS), B brake (CIRCLE),
    M jump (SQUARE), H (TRIANGLE), Backspace = START, U / O = L1 / R1,
    Y / P = L2 / R2.


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
Not everything of the original is ported yet (for example the
simultaneous two-player character select, 2-player tournament).
Such menu entries are shown greyed out. OPTIONS -> ADJUST SCREEN moves
the picture inside the window's black border (it cannot move when the
picture fills the window). VIBRATION is in SET CONTROLLERS, but this
build reads no gamepad (keyboard only), so there is nothing to rumble.


5. Licences
-----------
THIRD-PARTY-LICENSES.txt lists the open-source Rust crates compiled into
the program and their licences. Muppet RaceMania and its content belong
to their owners; this package contains none of it.
