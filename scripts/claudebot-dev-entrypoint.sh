#!/bin/bash
# claudebot dev entrypoint: start the md viewer + a cloudflared quick tunnel,
# announce the tunnel URL to the Discord channel, then exec the real command
# (claude). The announce flow runs in the background so claude starts
# immediately even if the tunnel is slow or fails.
{
    md >/tmp/md.log 2>&1 &
    MD_URL=""
    for _ in $(seq 50); do
        MD_URL=$(grep -om1 'http://127\.0\.0\.1:[0-9]*' /tmp/md.log) && break
        sleep 0.2
    done
    TUNNEL=""
    if [ -n "$MD_URL" ]; then
        cloudflared tunnel --no-autoupdate --url "$MD_URL" >/tmp/cloudflared.log 2>&1 &
        for _ in $(seq 150); do
            TUNNEL=$(grep -om1 'https://[a-zA-Z0-9-]*\.trycloudflare\.com' /tmp/cloudflared.log) && break
            sleep 0.2
        done
    fi
    if [ -n "$TUNNEL" ] && [ -n "$CLAUDEBOT_DISCORD_TOKEN" ] && [ -n "$CLAUDEBOT_CHANNEL_ID" ]; then
        curl -s -X POST "https://discord.com/api/v10/channels/$CLAUDEBOT_CHANNEL_ID/messages" \
            -H "Authorization: Bot $CLAUDEBOT_DISCORD_TOKEN" \
            -H "Content-Type: application/json" \
            -d "{\"content\":\"🌐 md viewer: $TUNNEL\"}" >/dev/null
    fi
} &
exec "$@"
