import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import DiscoveryNudge from './DiscoveryNudge';

const getPending = vi.fn();
const install = vi.fn();
const dismiss = vi.fn();
vi.mock('../../acp/discovery', () => ({
  getPendingDiscoveries: (...a: unknown[]) => getPending(...a),
  installDiscovery: (...a: unknown[]) => install(...a),
  dismissDiscovery: (...a: unknown[]) => dismiss(...a),
}));

const serverCard = {
  id: 'srv-mcp-io',
  kind: 'server',
  title: 'Model Context Protocol Docs',
  description: 'Search the modelcontextprotocol.io documentation.',
  sourceDomain: 'modelcontextprotocol.io',
  fixture: false,
  requiresAuth: false,
  serverName: 'io.modelcontextprotocol/docs',
  version: '1.0.0',
  endpoint: 'https://modelcontextprotocol.io/mcp',
  skills: [],
};

const authCard = {
  ...serverCard,
  id: 'srv-github',
  title: 'GitHub MCP Server',
  sourceDomain: 'api.github.com',
  fixture: true,
  requiresAuth: true,
  authKind: 'header',
  authInstructions: 'Paste a GitHub PAT with repo scope.',
  endpoint: 'https://api.githubcopilot.com/mcp/',
};

beforeEach(() => {
  getPending.mockReset();
  install.mockReset();
  dismiss.mockReset();
});

describe('DiscoveryNudge', () => {
  it('renders an auth-less server discovery and installs it into the session', async () => {
    getPending.mockResolvedValue({ mode: 'full', discoveries: [serverCard] });
    install.mockResolvedValue({
      kind: 'server',
      extensionName: 'docs-server',
      tools: ['search_model_context_protocol'],
      message: 'docs-server is now active in this session',
    });

    render(<DiscoveryNudge sessionId="s1" />);

    await waitFor(() => {
      expect(screen.getByText('MCP server discovered')).toBeInTheDocument();
      expect(screen.getByText('Model Context Protocol Docs')).toBeInTheDocument();
    });
    expect(getPending).toHaveBeenCalledWith('s1');
    expect(screen.getAllByText(/modelcontextprotocol\.io/).length).toBeGreaterThan(0);
    expect(screen.getByText(/https:\/\/modelcontextprotocol\.io\/mcp/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /install/i }));

    await waitFor(() => {
      expect(screen.getByText(/is now active in this session/)).toBeInTheDocument();
      expect(screen.getByText(/search_model_context_protocol/)).toBeInTheDocument();
    });
    expect(install).toHaveBeenCalledWith('s1', 'srv-mcp-io', undefined);
  });

  it('reveals a token field for auth-required servers and sends the secret on Connect', async () => {
    getPending.mockResolvedValue({ mode: 'full', discoveries: [authCard] });
    install.mockResolvedValue({
      kind: 'server',
      extensionName: 'github-mcp-server',
      tools: ['search_repositories'],
      message: 'github-mcp-server is now active in this session',
    });

    render(<DiscoveryNudge sessionId="s1" />);

    await waitFor(() => expect(screen.getByText('GitHub MCP Server')).toBeInTheDocument());
    expect(screen.getByText('auth required')).toBeInTheDocument();
    expect(screen.getByText('demo fixture')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /install/i }));
    const field = await screen.findByPlaceholderText(/auth token/i);
    expect(install).not.toHaveBeenCalled();

    fireEvent.change(field, { target: { value: 'ghp_demo_token' } });
    fireEvent.click(screen.getByRole('button', { name: /connect/i }));

    await waitFor(() =>
      expect(install).toHaveBeenCalledWith('s1', 'srv-github', 'ghp_demo_token')
    );
  });

  it('renders nothing when there are no discoveries', async () => {
    getPending.mockResolvedValue({ mode: 'light', discoveries: [] });
    const { container } = render(<DiscoveryNudge sessionId="s1" />);
    await waitFor(() => expect(getPending).toHaveBeenCalled());
    expect(container.querySelector('[data-testid="discovery-nudge"]')).toBeNull();
  });
});
