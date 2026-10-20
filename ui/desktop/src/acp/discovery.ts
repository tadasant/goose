import type {
  DiscoveryCardDto,
  DiscoveryInstallResponse_unstable as DiscoveryInstallResponse,
  DiscoveryPendingResponse_unstable as DiscoveryPendingResponse,
} from '@aaif/goose-acp-client';
import { getAcpClient } from './acpConnection';

export type { DiscoveryCardDto, DiscoveryInstallResponse, DiscoveryPendingResponse };

export async function getPendingDiscoveries(sessionId: string): Promise<DiscoveryPendingResponse> {
  const client = await getAcpClient();
  return client.goose.discoveryPending_unstable({ sessionId });
}

export async function installDiscovery(
  sessionId: string,
  id: string,
  secret?: string
): Promise<DiscoveryInstallResponse> {
  const client = await getAcpClient();
  return client.goose.discoveryInstall_unstable({ sessionId, id, secret });
}

export async function dismissDiscovery(sessionId: string, id: string): Promise<void> {
  const client = await getAcpClient();
  await client.goose.discoveryDismiss_unstable({ sessionId, id });
}
