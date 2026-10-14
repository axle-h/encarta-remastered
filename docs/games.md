# Games (plan Step 7)

The games and interactive extras of the disc, rebuilt in the web client (`web/src/games/`, route `/games`) over the
server's `/api/games/…` endpoints ([api.md](api.md#games), module `crates/encarta-server/src/games.rs`). Facts about
Encarta 2009 come from the reference VM ([vm-findings.md](vm-findings.md), private shots in `fixtures/geoquiz/`);
where the original behaviour is unknown, the choice made here is marked **(ours)**.

| Game | In Encarta 2009? | Data | Client |
|---|---|---|---|
| Geography Quiz | yes: Features, Games & Fun Stuff › Geography (feature item 701712336, `type="GeoQ"`) | `geoquiz_level`, `geoquiz_place` + gazetteer | `geoquiz/` |
| Quizzes | yes, inside the Flash board games (`BGAControl` `.iax`) | `quiz` (SWQUIZ) grouped by the boards' `questiongroups` | `quiz/` |
| Match games | yes, Flash `MatchGame` `.iax` (Games & Fun Stuff pages) | `interactive` + `iax_ref` media | `match/` |
| MindMaze | **no** (no feature record; its view URL closes Encarta) | `mindmaze_*`, `MINDMAZE.EIT` art | `mindmaze/` (Extras) |
| City Climate, Population Chart | **no** ("Page or keyword not found") | `out/media/interactives/*.json` | `charts/` (Extras) |

The Features menu lists Geography Quiz and Games & Quizzes, then an **Extras** group with MindMaze and the two charts.

## Geography Quiz

As observed in the VM: a start dialog (title "Geography Quiz", the word "quiz", "To begin the quiz, enter your name
and choose your level of difficulty.", Your Name, four radio buttons Navigator (easy), Explorer (not so easy),
Globetrotter (difficult), World Class (most difficult), OK/Cancel). A game is **20 questions**. The map shows the place:
a country (or continent) filled yellow on the Earth By Day picture, a city as a target marker; the left column reads
"Which country is this?" / "Which city is this?", the points on offer "(5 points)", **five answer buttons**,
"Question 1 of 20" and Exit Quiz. A wrong pick greys its button and the offer drops along `D*.TXT` line 2
(Navigator 5 → 4 …; World Class 25 → 20 → 10 …); the last entry (0) applies once four buttons are gone.

Server (`GET /api/games/geoquiz/{level}`):
- Questions per place type = `type_mix × 20`, largest remainders first; a pool too small for its share is filled from
  the other types. Places come from the gazetteer's `quiz_place` (all 1,372 resolve). The one D4 "country" that is a
  city (Vadodara, msworld.md §2) is asked as a place.
- **Choices (hypothesis).** `D*.TXT` line 3 (0 0 0 / 1 1 1 / 2 2 2 / 4 4 4 per type) is read as *how many of the four
  wrong answers come from the place's own region* (the pool's `group` column); the rest come from anywhere in the
  level's pool of that type. Both VM shots fit: Navigator Pakistan was offered with South Africa, Australia,
  New Zealand, Portugal (no regional share); World Class Hiroshima with Guangzhou, Beijing, Shanghai, Kōbe (all East
  Asia). Names are distinct; a tiny pool gives fewer buttons rather than repeats.
- Pool region quirks are kept as data (e.g. D1 files the United Kingdom under Oceania).

Client: the quiz runs on the shared `<Atlas quizMode initialStyle="atlas-quiz" />` (Encarta's quiz style: satellite,
borders, no names), plus a Hand/Pointer tool toggle. Quiz mode hides every name and the Read Article, Legend,
Customizer, Globe/Flat and style controls; the locator globe and zoom slider stay. The highlight is a fill on the `areas` layer filtered by `entity`; continent
areas are coarse outlines in the atlas data, so they are tinted at 42 % rather than filled **(ours)**. Countries fit
their bounding box (antimeridian-safe), cities a regional view (zoom 3.6) **(ours, close to the VM's 600-mile scale
bar)**. After a right pick the next question appears at once; after question 20 a "Scores" dialog opens (VM, vm-findings.md › UI parity pass). The end shows the score and a
per-level table of the ten best scores in this browser **(ours)**. Not done: the VM's full score table, Try Again
reopening the start dialog, the 9-tick slider look.

## Quizzes (SWQUIZ)

1,811 quizzes (1,113 `multichoice`, 698 `textAnswer`; the Kids copies are identical). In Encarta 2009 they are only
seen inside the ten `BGAControl` Flash board games ("Postcards from Africa", "Animal Paths", "Continental Quiz",
"Time Traveler", "United States State Capitals", "Spelling Bee", "Math Matrix", …), which name them in
`questiongroups/qgroup/@questions`; 1,726 ids are used, 85 by no board.

The client lists each board game (title, picture and caption from its launching item) with its question groups
(coded names such as `2_NoAm_Countries` are spelled out as "North America: Countries (round 2)") and plays a group:
question text, picture or sound, the answers (text, pictures or sounds) as radio buttons, **check answer**, then the
reward text and picture with **Media Credits** and **continue**; typed answers are compared after folding case,
accents, punctuation, a leading article and thousands separators **(ours)**. Points: 5 per right answer, the boards'
"Score +5" event. Media are resolved in each quiz's own folder (`dswMedia/quiz/{id}/`) through the manifest's
`iax_ref` rows, since file names repeat across folders.

**Not rebuilt: the board engine itself.** A BGA definition is a small game script: boards (SWF art with `navframe`
frames), pips with links and states, question settings per pip (`count`, `goal`, `time`, `usetimer`, `qgroups`),
events (`score`, `keys`, `dialog`, `newboard`, `newpip`, `endgame`, `retreat`, conditional `eval` on score/keys,
`labelfx`/`pipfx`/`boardfx`) and dialogs. Its art is Flash, so the faithful route is the disc's IAF player under Ruffle
(media viewer), and that route works: in the Ruffle test pass (web-client.md) all six boards tried open (Postcards
from Africa, United States State Capitals, Spelling Bee, Math Matrix, Wilderness Adventure, Sports Mountain); in
Postcards from Africa a postcard opens the Sahara board and a suitcase a SWQUIZ question with its picture (loaded from
`dswMedia/quiz/{id}/`), answers, reward text, Media Credits and continue; in Wilderness Adventure Start and a
destination lead to the Woods board.

**All ten boards played to an ending** (headless Chromium over the DevTools protocol, SwiftShader, harness kept out of
the repo). The harness reads each screen: it finds the answer buttons, the yellow continue / check answer button, the
scroll arrows and the text field by colour, looks up the open question's answer from the `quiz/{id}/{id}.iax` request,
and clicks pips at their definition positions. Answers can be forced right or wrong to reach both kinds of ending.

| Board (edition, id) | Run | Ending reached |
|---|---|---|
| United States State Capitals | Region 4, 11 stars, 4 right | fireworks "Keep playing to memorize all the state capitals!" |
| Sports Mountain (kids `00174d78`) | 10/10; again 0/10 | top: "You conquered Sports Mountain in style!" (100); low: "You made it to the top! Want to try again for a better score?" |
| Body of Knowledge (kids `00174d64`) | Student 14/14 with both side paths; Scientist 0/10 | "PERFECT SCORE in Doctor's Office" bonus, "… path completed", reward 3 "Great job!!" (64); reward 1 "Good effort. Want to try again for a better score?" |
| Animal Paths (kids `00174d68`) | Pets 14/14 | "PERFECT SCORE in Dog Town" (one bonus per game: the bonus adds the key that the other branch tests), monkey reward 3 "Great job!! You really went bananas!" (120) |
| Time Traveler (deluxe `00174d70`) | Ancient History 14/14 | "PERFECT SCORE in Ancient Egypt", fireworks "Great job!! You're a Master Historian!" (120) |
| Continental Quiz (deluxe `00174d74`) | Physical Features 7/7; Lightning Round 6 continents passed, Antarctica failed | fireworks "Fantastic job!! You rule the world!" (7; 6). The Lightning Round's instructions, 30-second timer, "correct answers needed" counter and pass/fail panels work |
| Spelling Bee (kids `00173a32`) | Level 1, typed words 10/10; a wrong word | "Way to Go!"; the first wrong word ends at once: "Try Again!" |
| Math Matrix (kids `00173c85`) | +/−: 17 cells, then the portal; ×: portal answered wrong | "You found it! … You only get one chance!", then "You did it! Want to play again?" (27); "Better luck next time." with every cell disabled |
| Wilderness Adventure (kids `00174d6c`) | Woods, Ocean, Mountains; again all wrong | "You did it!! … You win!", highest reward (parade) "Wow!! You're a hero!" (480); "You ran out of food three times in a row!", the rescue board (helicopter lifts the basket away) |
| Postcards from Africa (deluxe `001714ab`) | Sahara and Valley of the Kings, all suitcases | "You scored 200 points in just two stops! Way to go!" (200) |

No script error. Two requests fail, both harmless: Math Matrix's win sound `Matrix_Reward_01.mp3` is not on the disc
(the manifest's `iax_ref` has it unresolved), and Spelling Bee asks once for its own folder (an empty file name).
Long reveals (Continental Quiz) need the dialog's scroll arrow before continue shows; that is the original layout.

**Defect: picture answers are blank.** In a question whose answers are pictures, the four answer buttons are drawn
empty (hover lights them; clicking still answers). The JPEGs are requested and served (200), the same file shows as
the reward picture, and waiting does not help. Seen under Ruffle 0.7.1 and 0.8.0-nightly 2026-10-13. 31 board
questions are affected (Animal Paths 12, Wilderness Adventure 9, Postcards from Africa 8, Time Traveler 2), so those
can only be guessed in the board; the quiz player above shows them. Cause not yet found (PLAN).

The quiz player above remains the way to play a question group on its own.

## Match games

64 `MatchGame` interactives (13 Premium, 51 Kids). Each slide is a level (`@sequence`); `@itemsperbucket` items are
dealt at once (randomly when there are more); the top row is the `topbucket` (picture or text, with a hint in its
caption body, shown when picked), the bottom row the shuffled `bottombucket`s (text or picture, optional sound). Pick
one of each: a match turns the top card into its `solvedbucket` (picture and title); a miss flashes and counts a try.
The status line uses the definition's strings ("matches in", "tries", "Level:", "SCORE:", "Play Again!"). Scoring
**(ours)**: 10 per match; the reward line is the first text with no misses, the second with misses ≤ half the matches,
else the third.

The same player replaces Ruffle in the media viewer for `MatchGame` interactives (`games/GameInteractive.tsx`). Under
Ruffle 0.7.1 the `MatchGame` control's AVM1 scripts allocate without end (the stack shows a property setter running script)
until the wasm heap is exhausted (`handle_alloc_error`), and the player stops with nothing drawn; this is not the
script time limit, so raising `maxExecutionDuration` does not help.

## MindMaze (extra)

Encarta 2009 cannot open it; the question bank (8,020 questions, mindmaze.md) and the artwork, sounds and music of
`MINDMAZE.EIT` (90 pictures, 35 WAVs, 5 MIDIs) remain. The game is rebuilt around that art; its rules are **ours**
except where the art tells the story.

- **Screens.** `titlescr` (click to begin) → `logonscr` (name list; "click on the castle door to enter": the door is
  the hotspot) → for a new player the two story pages `open1`/`open2` → the castle → the stairway (`doortow`) between
  floors → one of the six endings `end1`–`end6` (random) in the throne room.
- **Castle.** Four floors, one per question level: 3×3, 4×3, 4×4 and 5×4 rooms. A floor is a random spanning tree
  plus about a third of the remaining walls as extra doors; the stairway is in the room farthest from the start.
  Floor 1 uses the dungeon walls (`dunrm`), the others the plaster room (`plnwalls`) with ceilings, floor coverings and
  furniture sprites placed in fixed slots. Sprites use pure cyan as the transparent colour (keyed on a canvas).
- **Doors and questions.** The room view shows the doors on the left, back and right walls (`doorlt`, `ndoor`,
  `doorrt`); above each hangs the picture of its subject (`pic1-n`/`pic2-n`, n = category + 1: the nine pictures
  match the nine categories). Knocking (`knock.wav`) brings a denizen (jester, king, queen, duke, ladies, servant,
  sorceress, armour, alchemist, travellers) and the question on the illuminated banner; the four answers are
  shuffled. Right: the door opens and you walk through (`right.wav`, footsteps). Wrong: the door is barred; if barred
  doors cut you off from the stairway, they are unbarred again.
- **Scoring.** +10 × level per right answer, −5 × level per wrong one (never below 0), +50 × level per floor; a
  reward per floor (`ribbon`, `medal`, `trophy`, `ring`; `award` is the empty frame).
- **Subjects.** The player may restrict the nine categories (labels are mindmaze.md's inferred names).
- **Answer anchors.** After each question the dialog links the correct answer's article, at the paragraph that
  answers the clue (below).
- **Sound.** Effects through `<audio>`; `intro.mid`, `bgloop1–3.mid` and `question.mid` through a small oscillator
  synth over the shared MIDI reader (drums left out). Sound and music can be switched off.
- **Players** (name, subjects, floor, score, rewards, questions seen, the floor in progress) are kept in this
  browser's `localStorage`; a returning player resumes the floor. Questions already seen are dealt last.
- Not used: `rmofdoor`, `atrium`, `grating`, `maninst`, `parrot` as a scene (it is a prop), `match1/2.wav`,
  `birds.wav`, and the `Area0–8.lst` article lists (the converter omits them; below).

### Answer anchors and the `location` word

mindmaze.md's hypothesis ("paragraph number in the answer article") checked against `paragraph` (all 764 questions with
`location` > 1 whose article exists): for 348 the paragraph `location` of the 2009 article is the best match for the
clue and answer words, and for 30 more it is within one word of the best; the rest have drifted, monotonically (the
China questions 163 … 258 now sit at paragraphs ≈ 209 … 357). Conclusion: `location` is a 1-based paragraph id in an
**earlier revision** of the article. The server therefore anchors by content: each paragraph scores one point per clue
word and two per answer word it shares (lower-case runs of 3+ letters, common words dropped); the best wins, ties go
to the paragraph nearest `location`; none sharing a word → the article top. Suggested addition to mindmaze.md.

`MINDMAZE.EIT/Area0–8.lst`: one per category, lists of 509–1,461 article ids; 91–98 % of each category's
correct-answer articles are in its list, plus related articles. Probably a "learn more" pool; unused.

## City Climate and Population Chart (extras)

Rebuilt from the Director datasets (shockwave.md §2.2, §7) as SVG charts with the movies' controls.

- **City Climate**: measure buttons in two groups (Temperature: Annual Average, January High/Low, July High/Low;
  Precipitation: Annual Average, Days with Precip., Avg. Snowfall), Imperial/Metric, eight city slots each a menu
  ("< add a city >", and "< remove a city >" once filled), "na = Data not available", the intro narration
  (`clim_in.swa` → MP3/Opus). Bars start at zero; the axis runs from min(`axis_min`, 0) to `axis_max` with ticks on the
  ends **(ours: zero baseline)**. Defaults Washington, D.C., London and Tokyo **(ours)**.
- **Population Chart**: United States / Canada, Total Population / Population density / Percentage Urban (the
  dataset's "Percantage" is corrected on screen), Imperial/Metric (per country, defaults from the data), four series
  menus ("< add a state >" / "< add a province >"), the preset y scales (smallest that fits the selection), the
  footnote and narration. Lines with point markers, a hover readout per census year.
- Both have a table view. Series colours are fixed per slot (validated categorical palette).
- Launch filters are not used: none of their ids exist in 2009 (vm-findings.md).

## Tests

`cargo test -p encarta-server --lib games` (synthetic databases under the cargo target dir) and
`web/src/games/games.test.ts{,x}` (castle, answers, quiz rules, ticks and scales, match parsing; quiz player, climate
chart, match board, quiz dialog in jsdom).
