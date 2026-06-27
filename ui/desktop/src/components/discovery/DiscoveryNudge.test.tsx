import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import DiscoveryNudge from './DiscoveryNudge';

// Mock the shared hey-api client the nudge talks to.
const get = vi.fn();
const post = vi.fn();
vi.mock('../../api/client.gen', () => ({
  client: {
    get: (...a: unknown[]) => get(...a),
    post: (...a: unknown[]) => post(...a),
  },
}));

const serverCard = {
  id: 'srv-mcp-io',
  kind: 'server',
  title: 'Model Context Protocol Docs',
  description: 'Search the modelcontextprotocol.io documentation.',
  source_domain: 'modelcontextprotocol.io',
  fixture: false,
  icon: null,
  requires_auth: false,
  auth_kind: null,
  auth_instructions: null,
  server_name: 'io.modelcontextprotocol/docs',
  version: '1.0.0',
  endpoint: 'https://modelcontextprotocol.io/mcp',
  skills: [],
};

const authCard = {
  ...serverCard,
  id: 'srv-github',
  title: 'GitHub MCP Server',
  source_domain: 'github.com',
  fixture: true,
  requires_auth: true,
  auth_kind: 'token',
  auth_instructions: 'Paste a GitHub PAT with repo scope.',
  endpoint: 'https://api.githubcopilot.com/mcp/',
};

beforeEach(() => {
  get.mockReset();
  post.mockReset();
});

describe('DiscoveryNudge', () => {
  it('renders an auth-less server discovery and installs it into the session', async () => {
    get.mockResolvedValue({ data: { mode: 'full', discoveries: [serverCard] } });
    post.mockResolvedValue({
      data: {
        ok: true,
        kind: 'server',
        extension_name: 'model_context_protocol_docs',
        tools: ['search_model_context_protocol'],
        message: 'model_context_protocol_docs is now active in this session',
      },
    });

    render(<DiscoveryNudge sessionId="s1" />);

    await waitFor(() => {
      expect(screen.getByText('MCP server discovered')).toBeInTheDocument();
      expect(screen.getByText('Model Context Protocol Docs')).toBeInTheDocument();
    });
    // source + endpoint surfaced (domain appears in both the source line and endpoint)
    expect(screen.getAllByText(/modelcontextprotocol\.io/).length).toBeGreaterThan(0);
    expect(screen.getByText(/https:\/\/modelcontextprotocol\.io\/mcp/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /install/i }));

    await waitFor(() => {
      expect(screen.getByText(/is now active in this session/)).toBeInTheDocument();
      expect(screen.getByText(/search_model_context_protocol/)).toBeInTheDocument();
    });

    const installCall = post.mock.calls.find(
      (c) => (c[0] as { url: string }).url === '/discovery/install'
    );
    expect(installCall).toBeTruthy();
    expect((installCall![0] as { body: { id: string } }).body.id).toBe('srv-mcp-io');
  });

  it('reveals a token field for auth-required servers and posts the secret on Connect', async () => {
    get.mockResolvedValue({ data: { mode: 'full', discoveries: [authCard] } });
    post.mockResolvedValue({
      data: {
        ok: true,
        kind: 'server',
        extension_name: 'github_mcp_server',
        tools: ['search_repositories'],
        message: 'github_mcp_server is now active in this session',
      },
    });

    render(<DiscoveryNudge sessionId="s1" />);

    await waitFor(() => expect(screen.getByText('GitHub MCP Server')).toBeInTheDocument());
    expect(screen.getByText('auth required')).toBeInTheDocument();
    expect(screen.getByText('demo fixture')).toBeInTheDocument();

    // First click reveals the password field (no install yet).
    fireEvent.click(screen.getByRole('button', { name: /install/i }));
    const field = await screen.findByPlaceholderText(/auth token/i);
    expect(post).not.toHaveBeenCalledWith(
      expect.objectContaining({ url: '/discovery/install' })
    );

    fireEvent.change(field, { target: { value: 'ghp_demo_token' } });
    fireEvent.click(screen.getByRole('button', { name: /connect/i }));

    await waitFor(() => {
      const call = post.mock.calls.find(
        (c) => (c[0] as { url: string }).url === '/discovery/install'
      );
      expect(call).toBeTruthy();
      expect((call![0] as { body: { secret: string } }).body.secret).toBe('ghp_demo_token');
    });
  });

  it('renders nothing when there are no discoveries', async () => {
    get.mockResolvedValue({ data: { mode: 'light', discoveries: [] } });
    const { container } = render(<DiscoveryNudge sessionId="s1" />);
    await waitFor(() => expect(get).toHaveBeenCalled());
    expect(container.querySelector('[data-testid="discovery-nudge"]')).toBeNull();
  });
});
