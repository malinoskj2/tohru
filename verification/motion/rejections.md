# Animation failure checklist

Every item below is a fault the owner rejected in a Tohru animation, worded as a check. Walk the
whole list for every motion change, including items that seem unrelated to it. Owner quotes are
lightly trimmed. The **Check** column names the `tools/verify/motion.py` check that measures the item,
or says what to look at when no automatic check exists.

Sources: owner messages in Orca chats and voice (2026-11-11 to 2026-11-25), bot item 17
"Fix Rive dragon head bulge and independently review", the AFK child's `issues.md`, and
`tools/dragon/README.md` (landing acceptance note).

## Body continuity

| ID | Fault | What the owner said | Check |
| --- | --- | --- | --- |
| B1 | Body morphs between separate drawings instead of moving into the next pose | Flight settle "looks like she's morphing between 3 different shapes … her body is supposed to be turning … the AI typical lack of continuity is very evident" (11-22). Landings accepted only provisionally because the settle "still visibly reads as the dragon morphing into her idle pose". | `pose-snap`, `silhouette-area`; onion sheet: outlines should step evenly |
| B2 | She becomes a different creature partway through | Tween comparison: "it looks like it transitions into a different being" (11-21) | Reviewer: compare first, middle and last frames for identity (horns, face, proportions) |
| B3 | Colours shift between frames | "I don't like the color shifts wan and tooncrafter are doing" (11-21) | `part-area` on colour parts; `difference` sheet |
| B4 | Part of the head or body swells and shrinks back (volume pulse) | "The head kind of bulges on the left as the dragon turns around" (11-23); after a repair: "It still has the issue" (item 17, 11-24) | `part-area` on zones and named parts, `outline-pulse`, `silhouette-area` |
| B5 | Repairs leave paint seams or notches | Item 17 trials rejected for "visible paint seams on the left temple" and a notched horn base | Reviewer: 100% crops of repaired regions in every changed frame |
| B6 | An extra shape seems attached to a limb when it moves | "Reads as there being an extra artifact attached to the arm when it moves" (11-22) | `outline-pulse`; onion skin around the limb |
| B7 | Warping the body or wings instead of drawing each state | "We shouldn't be using warp effects on its wings or body. We should generate unique art for each state" (11-11) | Reviewer: stretched texture or smeared highlights in the difference sheet |

## Turns and facing

| ID | Fault | What the owner said | Check |
| --- | --- | --- | --- |
| T1 | Facing flips in one frame instead of turning | "When she turns she just changes from 1 direction to the other in 1 frame. We need a believable turn with her body rotating" (11-22); corner nap "alternates direction instantly … in one frame" (11-25) | `mirror-flip`, `pose-snap` |
| T2 | A blank or missing frame while switching art | Found with the T1 clips: tail chase has empty frames at every turn | `vanish` |
| T3 | A pose change happens in one frame instead of a transition | Sleepy drift "unconvincingly transitions into the sleep pose in 1 frame from her flight animation" (11-22) | `part-area` (art swap), `pose-snap` |

## Face

| ID | Fault | What the owner said | Check |
| --- | --- | --- | --- |
| F1 | An eye disappears or seems to wink during a move | "His right eye (from our perspective) just disappears" (11-22); "it kind of looks like her right eye … is winking as she turns" (11-23) | `part-area` on the dark eye colour; reviewer crops of both eyes |
| F2 | Mouth action reads wrongly | "Looks like he's opening his mouth to yawn then just plugging it with her hand" (11-22); "the mouth needs to scale and articulate slightly more" (11-21) | Reviewer |

## Motion quality

| ID | Fault | What the owner said | Check |
| --- | --- | --- | --- |
| M1 | A static drawing slides, bobs or zooms instead of moving | Hide and peek "reads as her just shifting out in a static animation instead of peeking out in a playful way" (11-22); "she's just a static image translating … we need more frames" (11-25); window approach "looks like she just scales up" (11-25) | `rigid-motion` |
| M2 | Big gestures that add nothing | "Lose the big stretch completely" (11-22) | Reviewer |
| M3 | Too few drawings for the motion | "The walking animation probably needs more frames"; hide and peek "needs more frames" | Filmstrip: count distinct drawings; `rigid-motion` |

## Staging on the desktop

| ID | Fault | What the owner said | Check |
| --- | --- | --- | --- |
| S1 | A body meant to rest on a screen edge lifts off it, so it looks cut off | Glass inspection: "since we are wobbling the image of her up and down we see a space between the bottom of her body and the bottom of the desktop. It looks like her body is just chopped in half" (11-22, again 11-25) | `edge-contact` |
| S2 | Occlusion is wrong | Window perch: "her tail should be not visible because it is behind the window" (11-22) | Reviewer on `desktop.mp4` frames |
| S3 | Depth and scale disagree with nearby windows | Distant flight: "she shouldn't be that far away while on top of a window … the depth of the window should be close" (11-22) | Reviewer |
| S4 | Too small, or enlarged from small art | Upside-down peek "should be at screen size scale" and "she's too small" (11-22, 11-25); enlarging a small atlas cell is not acceptable | Reviewer at real desktop size; check source resolution against display size |
| S5 | She exits or returns to the corner when she should linger | "She shouldn't exit to the right unless it's just for the demo, she should linger" (11-22); "at the end of it she moves back to the bottom right … is that how it will actually look" (11-25) | Reviewer: last second of the clip |

## Reading the checks

A strong flag is a measured discontinuity, not proof of a fault, and a clean report is not
approval. Approved native landings also raise flags (entry from the screen edge, wingbeats), which
is why reports compare each check with the approved clip. The owner's own judgement stays final.
