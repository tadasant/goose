import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import DiscoveryNudge from './DiscoveryNudge';
import { IntlTestWrapper } from '../../i18n/test-utils';

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

    render(<DiscoveryNudge sessionId="s1" />, { wrapper: IntlTestWrapper });

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

    render(<DiscoveryNudge sessionId="s1" />, { wrapper: IntlTestWrapper });

    await waitFor(() => expect(screen.getByText('GitHub MCP Server')).toBeInTheDocument());
    expect(screen.getByText('auth required')).toBeInTheDocument();
    expect(screen.getByText('demo fixture')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /install/i }));
    const field = await screen.findByPlaceholderText(/auth token/i);
    expect(install).not.toHaveBeenCalled();

    fireEvent.change(field, { target: { value: 'ghp_demo_token' } });
    fireEvent.click(screen.getByRole('button', { name: /connect/i }));

    await waitFor(() => expect(install).toHaveBeenCalledWith('s1', 'srv-github', 'ghp_demo_token'));
  });

  it('renders nothing when there are no discoveries', async () => {
    getPending.mockResolvedValue({ mode: 'light', discoveries: [] });
    const { container } = render(<DiscoveryNudge sessionId="s1" />, { wrapper: IntlTestWrapper });
    await waitFor(() => expect(getPending).toHaveBeenCalled());
    expect(container.querySelector('[data-testid="discovery-nudge"]')).toBeNull();
  });
  it('shows the background-discovery affordance and opens the full trace', async () => {
    const trace = {
      id: 't1',
      status: 'done',
      startedAt: new Date().toISOString(),
      model: 'openai / gpt-4o-mini',
      requests: ['go to my GitHub and summarise my open PRs'],
      systemPrompt: 'You are goose\'s background AI Catalog discovery reasoner.',
      prompt: 'Recent user requests',
      rawOutput: '{"intent":"summarise the user\'s open GitHub PRs","domains":["github.com"]}',
      intent: "summarise the user's open GitHub PRs",
      rationale: 'GitHub publishes an official MCP server',
      candidateDomains: ['github.com'],
      probes: [
        {
          domain: 'github.com',
          url: 'https://github.com/.well-known/ai-catalog.json',
          outcome: '1 entr(y/ies) in live ai-catalog.json',
          found: ['MCP server: GitHub'],
        },
      ],
      decision: 'suggest installing MCP server: GitHub (nudge only)',
    };
    getPending.mockResolvedValue({
      mode: 'off',
      backgroundEnabled: true,
      backgroundRunning: false,
      traces: [trace],
      discoveries: [
        {
          ...serverCard,
          id: 'gh',
          title: 'GitHub',
          sourceDomain: 'github.com',
          track: 'background',
          intent: trace.intent,
          traceId: 't1',
        },
      ],
    });

    render(<DiscoveryNudge sessionId="s1" />, { wrapper: IntlTestWrapper });

    const status = await screen.findByTestId('background-discovery-status');
    expect(status).toHaveTextContent('suggest installing MCP server: GitHub');
    expect(screen.getByText(/background · summarise the user's open GitHub PRs/)).toBeInTheDocument();

    fireEvent.click(status);
    const dialog = await screen.findByTestId('discovery-trace');
    expect(dialog).toHaveTextContent('GET https://github.com/.well-known/ai-catalog.json');
    expect(dialog).toHaveTextContent('go to my GitHub and summarise my open PRs');
  });

  it('shows a running indicator while the background reasoner works', async () => {
    getPending.mockResolvedValue({
      mode: 'off',
      backgroundEnabled: true,
      backgroundRunning: true,
      traces: [
        {
          id: 't2',
          status: 'reasoning',
          startedAt: new Date().toISOString(),
          model: 'm',
          requests: ['x'],
          systemPrompt: '',
          prompt: '',
          rawOutput: '',
          candidateDomains: [],
          probes: [],
          decision: 'reasoning about intent…',
        },
      ],
      discoveries: [],
    });
    render(<DiscoveryNudge sessionId="s1" />, { wrapper: IntlTestWrapper });
    const status = await screen.findByTestId('background-discovery-status');
    expect(status).toHaveAttribute('data-running', 'true');
    expect(status).toHaveTextContent('reasoning about intent');
  });
});
