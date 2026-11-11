# Demo video v2 (open-video-demo project)

`story.json` and `video.json` are the inputs for `~/open-video-demo` (`./scripts/new.sh revenant-demo`, copy both files in, then `./scripts/run.sh --project revenant-demo`).

The page scenes point at `http://127.0.0.1:PORT (any free port, for example 18110)/`: the same build as the deployed app (https://revenant-kiroween.fly.dev), served locally with `PORT=18110 REVENANT_FIXTURES=fixtures REVENANT_WEB_DIR=apps/web/out ./target/debug/revenant-api`. Reason: the recording machine's own network interface flaps (`ERR_NETWORK_CHANGED`), which broke takes against the public URL. The live scenes still query the real Internet Archive. To record against the public site, replace the `src` values with the Fly URL and warm the machine first.

Output: 1920x1080, 145 s, narrated, subtitles burned in. The old v1 scripts in this folder's parent are superseded.
