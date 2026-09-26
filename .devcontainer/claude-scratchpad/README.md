# Linux Claude scratchpad probe

This image runs Claude Code 2.1.280 on Debian Bookworm as uid 1000. It is
separate from the Tungsten devcontainer so its build does not change normal
development setup. No credentials or workspace files are copied into the image.

```sh
docker build -f .devcontainer/claude-scratchpad/Dockerfile \
  -t tungsten-claude-scratchpad-probe:2.1.280 .
docker run -d --name tungsten-claude-scratchpad-probe \
  --mount type=volume,source=tungsten-claude-scratchpad-tmp,target=/tmp \
  tungsten-claude-scratchpad-probe:2.1.280 sleep infinity
docker exec -it tungsten-claude-scratchpad-probe claude auth login
```

Complete the browser login in your own terminal. Then start a real session and
inspect the root without printing credentials:

```sh
docker exec tungsten-claude-scratchpad-probe \
  claude -p 'Reply OK' --no-session-persistence
docker exec tungsten-claude-scratchpad-probe sh -lc \
  'id; readlink -f /tmp; stat -c "%n uid=%u mode=%a" /tmp/claude-$(id -u)'
```

The named `/tmp` volume preserves the observed directory across invocations.
Authentication remains in the container's writable layer, outside the image and
outside the repository.

On 2026-09-26, the authenticated session returned `OK`. The inspection command
reported `uid=1000(probe)`, canonical `/tmp`, and
`/tmp/claude-1000 uid=1000 mode=700`. Before the first CLI invocation,
`/tmp/claude-1000` was absent. This records the observed host contract for
this Debian container; it does not claim that every Linux integration uses the
same root.

When finished, remove the container and volume:

```sh
docker rm -f tungsten-claude-scratchpad-probe
docker volume rm tungsten-claude-scratchpad-tmp
```
