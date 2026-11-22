# Tohru Discord avatar concepts

Ten individual original raster generations, created 2026-11-22 with the built-in image_gen.imagegen tool in ten calls (one per variant), with no retries or post-generation image edits. All are 1254 × 1254 PNG RGB, opaque, approximately 1.4–1.8 MiB each; exact bytes, dimensions, SHA-256, prompts and original tool output paths are in metadata.json. The request aimed at 1024 square; the built-in tool returned 1254 square. Model revision and seed are not exposed by the tool.

## Review

- Live Needs review gallery: http://192.168.1.131:18085/
- Square/circular comparison: http://192.168.1.131:18086/
- Gallery #1–#10 correspond exactly to avatar-01–avatar-10.
- Ask the human which avatar they prefer by #id.
- Strongest options: #3 Delighted has the clearest small-size expression; #7 Paws under chin combines a large face with a warm distinctive pose; #2 Three-quarter greeting adds a welcoming gesture while keeping the eyes legible.
- #1, #5, #6, #8 and #9 show more of the body, so their facial expression is subtler at 32px. #8 emphasizes the wing silhouette; #9 emphasizes the circular tail. Several prompts requested busts but generation retained full bodies; captions describe the actual images.

## Character verification

Reference copied read-only from voice-dragon-3d-exploration/.tmp/dragon-3d/attempt-26/reference-165.png. Its decoded RGBA pixels match legacy-body.webp canonical cell 165 exactly. Reference SHA-256: 48176c5c207c45cf99032fde1dd6a1e488cb5db615d3479b2b868ea0df6dbf22.

Read tools/dragon/art/poses.json, tools/dragon/pack.py and shell/assistant/dragon/README.md before generation. Current body.webp is 5760 square: 24 columns of 240px cells; cell 165 is column 21 row 6, top-left (5040,1440). The packer preserves legacy indices 0–201, resizing their 300px source cells to 240px. poses.json registers the newer hover keys, not a different replacement for canonical 165.

## Validation

All ten originals inspected individually at full size. Orca browser comparison checked their square originals and CSS circular masks at 128/64/32px on light/dark backgrounds (review-01.png through review-03.png). Faces and horn silhouettes remain recognizable within the circular crops; no broken or extra limbs, text or unrelated props were accepted. No retries were needed. Browser confirmed all 70 comparison image elements loaded, and names/source paths match 01–10. Gallery confirmed all 10 originals loaded; mobile 390px viewport has 390px document width (gallery-mobile.png). Browser page ID cddb91f9-617d-4453-b1be-f73e0e8dd807.

## Preservation before cleanup

Asset root on host:
`/home/jesse/orca/workspaces/tohru/discord-avatar-designs/.tmp/discord-avatars/`

Preserve this complete directory before removing this child. The required assets are the ten avatar-*.png originals, reference-165.png, prompts.json, metadata.json, gallery-batch.json, index.html, build_preview.py, README.md, preservation-manifest.json, review-01.png through review-03.png, gallery-mobile.png and gallery/ state. Tool originals also remain under /home/jesse/.codex/generated_images/01a20558-d99c-71a0-bb00-44d176b786b0/; stable copies here have identical bytes. The preservation manifest maps absolute host source paths to relative destinations and SHA-256.

Parent should copy the directory to its own .tmp/discord-avatars before child cleanup, rewrite gallery-batch.json absolute paths to the preserved parent copies, then publish with media-gallery batch in its own gallery (do not init/reset an existing parent gallery). The HTML uses relative image links and works immediately after the directory copy. Serve from the parent's reserved preview ports and give the human the new published URLs. Child URLs live only while this workspace/server remains running.

Small source docs and the exact prompts are committed under docs/discord-avatar-concepts/; generated media stays untracked in .tmp. No application art, bot configuration or credentials were touched, and no Discord API operation was made.

