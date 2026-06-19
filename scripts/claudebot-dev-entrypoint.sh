#!/bin/bash
# claudebot dev entrypoint: start the md viewer + a cloudflared quick tunnel,
# announce the tunnel URL to the Discord channel, then exec the real command
# (claude). The announce flow runs in the background so claude starts
# immediately even if the tunnel is slow or fails.

# Headless display for the SDL/GL demos. Xvfb gives SDL a virtual
# framebuffer (DISPLAY=:99, set in the image), fluxbox maps windows, and
# x11vnc serves it on :5900 so you can watch the demos render live. All
# best-effort: nothing here may block `claude` from starting.
export DISPLAY="${DISPLAY:-:99}"
{
    Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >/tmp/xvfb.log 2>&1 &
    for _ in $(seq 50); do xdpyinfo -display "$DISPLAY" >/dev/null 2>&1 && break; sleep 0.2; done
    fluxbox >/tmp/fluxbox.log 2>&1 &
    x11vnc -display "$DISPLAY" -nopw -forever -shared -bg -quiet >/tmp/x11vnc.log 2>&1 || true
} &

{
    MD_URL="http://127.0.0.1:8085"
    # serve the project directory (docker -w puts us there); flags go first
    md -depth 5 -listen 0.0.0.0:8085 . >/tmp/md.log 2>&1 &
    for _ in $(seq 50); do
        curl -s -o /dev/null --max-time 2 "$MD_URL/" && break
        sleep 0.2
    done
    # http2: QUIC suffers from UDP buffer limits inside containers (tunnel
    # registers but the data path stalls)
    cloudflared tunnel --no-autoupdate --protocol http2 --url "$MD_URL" \
        >/tmp/cloudflared.log 2>&1 &
    TUNNEL=""
    for _ in $(seq 150); do
        TUNNEL=$(grep -om1 'https://[a-zA-Z0-9-]*\.trycloudflare\.com' /tmp/cloudflared.log) && break
        sleep 0.2
    done
    READY=""
    if [ -n "$TUNNEL" ]; then
        # The subdomain is brand-new: querying it too early gets an NXDOMAIN
        # that resolvers negative-cache, poisoning every later retry. Give
        # Cloudflare DNS a moment, then check over DoH to bypass the local
        # resolver chain entirely.
        sleep 10
        for _ in $(seq 60); do
            [ "$(curl -s -o /dev/null -w '%{http_code}' --max-time 8 \
                  --doh-url https://1.1.1.1/dns-query "$TUNNEL/")" = "200" ] && READY=1 && break
            sleep 2
        done
    fi
    if [ -n "$READY" ] && [ -n "$CLAUDEBOT_DISCORD_TOKEN" ] && [ -n "$CLAUDEBOT_CHANNEL_ID" ]; then
        curl -s -X POST "https://discord.com/api/v10/channels/$CLAUDEBOT_CHANNEL_ID/messages" \
            -H "Authorization: Bot $CLAUDEBOT_DISCORD_TOKEN" \
            -H "Content-Type: application/json" \
            -d "{\"content\":\"🌐 md viewer: $TUNNEL\"}" >/dev/null
    fi
} &

# Semfora: regenerate the workspace index (the ~/.cache index does not survive
# restarts) and start the FS-monitor daemon. The MCP server itself is launched
# on demand by claude (`semfora-engine serve`); here we only need a fresh index
# and the daemon. Best-effort; never blocks claude.
{
    if command -v semfora-engine >/dev/null 2>&1; then
        semfora-engine index generate . >/tmp/semfora-index.log 2>&1 || true
        semfora-daemon >/tmp/semfora-daemon.log 2>&1 &
    fi
} &

exec "$@"
