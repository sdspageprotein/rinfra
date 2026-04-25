[简体中文](registry-sdk-troubleshooting.zh-CN.md)

# Registry SDK Troubleshooting

## Connection Failure

Symptoms:

- `connection refused`
- `timeout`

Checklist:

1. Confirm main node cluster TCP service is running.
2. Confirm `mainAddress` points to cluster TCP address, not admin HTTP address.
3. Confirm firewall/security group/network policy allows the port.

## Authentication Failure

Symptoms:

- `RegisterAck.success = false`
- logs include `invalid token`

Checklist:

1. Ensure SDK `clusterToken` matches main `cluster_token`.
2. Ensure token has no leading/trailing spaces.
3. Auth failure is fail-fast in V1, fix config and restart.

## Endpoint Unreachable

Symptoms:

- node appears online in main, but endpoint is not reachable.

Checklist:

1. Verify you are not advertising `127.0.0.1` or `0.0.0.0`.
2. Verify advertised address is routable from main/workers.
3. In Kubernetes, prefer Service DNS.

## Heartbeat / Reconnect Issues

Symptoms:

- frequent Offline/Online flapping
- connection does not recover after disconnection

Checklist:

1. Check network stability and TCP idle timeout settings.
2. Check SDK reconnect state and backoff logs.
3. Check whether main is restarting frequently or overloaded.
