import { useCallback, useEffect, useRef, useState } from 'react';
import { Boxes, Sparkles, ShieldCheck, X, Loader2, Check, BrainCircuit } from 'lucide-react';
import {
  dismissDiscovery,
  getPendingDiscoveries,
  installDiscovery,
  type DiscoveryCardDto as DiscoveryCard,
  type DiscoveryTraceDto,
} from '../../acp/discovery';
import DiscoveryTraceDialog from './DiscoveryTraceDialog';
import { Button } from '../ui/button';
import { cn } from '../../utils';

type RowState =
  | { phase: 'idle' }
  | { phase: 'auth' }
  | { phase: 'installing' }
  | { phase: 'done'; message: string; tools: string[] }
  | { phase: 'error'; message: string };

/**
 * Background-discovery install nudge. Rendered between the transcript and the
 * prompt input. Polls the server for pending discoveries (MCP servers / Skills
 * bundles found by probing domains the session touched) and lets the user
 * install them into the *current* session with one click.
 */
export default function DiscoveryNudge({ sessionId }: { sessionId: string }) {
  const [cards, setCards] = useState<DiscoveryCard[]>([]);
  const [rowState, setRowState] = useState<Record<string, RowState>>({});
  const [tokens, setTokens] = useState<Record<string, string>>({});
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const rowStateRef = useRef(rowState);
  rowStateRef.current = rowState;
  const [backgroundEnabled, setBackgroundEnabled] = useState(false);
  const [backgroundRunning, setBackgroundRunning] = useState(false);
  const [traces, setTraces] = useState<DiscoveryTraceDto[]>([]);
  const [openTraceId, setOpenTraceId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!sessionId) return;
    try {
      const pending = await getPendingDiscoveries(sessionId);
      const { discoveries } = pending;
      setBackgroundEnabled(pending.backgroundEnabled ?? false);
      setBackgroundRunning(pending.backgroundRunning ?? false);
      setTraces(pending.traces ?? []);
      setCards((prev) => {
        // Keep cards that are mid-flow (installing/done/error) even if the
        // server already cleared them, so the user sees the result.
        const sticky = prev.filter((c) => {
          const st = rowStateRef.current[c.id]?.phase;
          return st === 'done' || st === 'installing' || st === 'error';
        });
        const seen = new Set(sticky.map((c) => c.id));
        const merged = [...sticky];
        for (const d of discoveries) {
          if (!seen.has(d.id)) merged.push(d);
        }
        return merged;
      });
    } catch {
      // Backend not ready / method absent — stay silent, this is a sidecar.
    }
  }, [sessionId]);

  useEffect(() => {
    refresh();
    pollRef.current = setInterval(refresh, 1200);
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [refresh]);

  const removeCard = (id: string) => setCards((prev) => prev.filter((c) => c.id !== id));

  const setPhase = (id: string, s: RowState) => setRowState((prev) => ({ ...prev, [id]: s }));

  const dismiss = async (card: DiscoveryCard) => {
    removeCard(card.id);
    try {
      await dismissDiscovery(sessionId, card.id);
    } catch {
      /* best-effort */
    }
  };

  const install = async (card: DiscoveryCard) => {
    // Auth-required servers: reveal the token field first.
    if (card.kind === 'server' && card.requiresAuth && card.authKind !== 'oauth') {
      if (rowState[card.id]?.phase !== 'auth') {
        setPhase(card.id, { phase: 'auth' });
        return;
      }
      if (!(tokens[card.id] || '').trim()) {
        setPhase(card.id, { phase: 'error', message: 'Enter a token to continue.' });
        return;
      }
    }

    setPhase(card.id, { phase: 'installing' });
    try {
      const data = await installDiscovery(sessionId, card.id, tokens[card.id] || undefined);
      setPhase(card.id, {
        phase: 'done',
        message: data.message,
        tools: data.tools ?? [],
      });
      // Clear the result chip after a few seconds and refetch.
      setTimeout(() => {
        removeCard(card.id);
        refresh();
      }, 6000);
    } catch (e) {
      setPhase(card.id, {
        phase: 'error',
        message: e instanceof Error ? e.message : 'Install failed.',
      });
    }
  };

  const latestTrace = traces.length ? traces[traces.length - 1] : null;
  const openTrace = traces.find((t) => t.id === openTraceId) ?? null;
  const showAffordance = backgroundEnabled && latestTrace !== null;

  if (cards.length === 0 && !showAffordance) return null;

  return (
    <div className="mx-4 mb-2 flex flex-col gap-2" data-testid="discovery-nudge">
      {openTrace && <DiscoveryTraceDialog trace={openTrace} onClose={() => setOpenTraceId(null)} />}
      {showAffordance && latestTrace && (
        <button
          type="button"
          data-testid="background-discovery-status"
          data-running={backgroundRunning ? 'true' : 'false'}
          onClick={() => setOpenTraceId(latestTrace.id)}
          className={cn(
            'flex w-fit max-w-full items-center gap-2 rounded-full border px-3 py-1 text-xs transition-colors',
            backgroundRunning
              ? 'border-violet-500/40 bg-violet-500/10 text-violet-600 dark:text-violet-300'
              : 'border-borderSubtle bg-background-secondary/60 text-text-secondary hover:text-text-primary'
          )}
          title="Open the background discovery trace"
        >
          {backgroundRunning ? (
            <Loader2 className="size-3.5 shrink-0 animate-spin" />
          ) : (
            <BrainCircuit className="size-3.5 shrink-0" />
          )}
          <span className="truncate">
            {backgroundRunning
              ? `Background discovery: ${latestTrace.decision}`
              : `Background discovery · ${latestTrace.decision}`}
          </span>
          <span className="shrink-0 underline underline-offset-2">trace</span>
        </button>
      )}
      {cards.map((card) => {
        const st = rowState[card.id]?.phase ?? 'idle';
        const isServer = card.kind === 'server';
        return (
          <div
            key={card.id}
            className={cn(
              'rounded-xl border border-borderSubtle bg-background-secondary/60 px-4 py-3',
              'shadow-sm animate-[fadein_300ms_ease-in_forwards]'
            )}
            data-discovery-kind={card.kind}
          >
            <div className="flex items-start gap-3">
              <div className="mt-0.5 shrink-0">
                {card.icon ? (
                  <img src={card.icon} alt="" className="size-8 rounded-md bg-white/80 p-1" />
                ) : isServer ? (
                  <Boxes className="size-7 text-text-secondary" />
                ) : (
                  <Sparkles className="size-7 text-text-secondary" />
                )}
              </div>

              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="text-[11px] font-semibold uppercase tracking-wide text-text-secondary">
                    {isServer ? 'MCP server discovered' : 'Skills bundle discovered'}
                  </span>
                  {card.fixture ? (
                    <span className="rounded-full bg-background-muted px-2 py-0.5 text-[10px] text-text-secondary">
                      demo fixture
                    </span>
                  ) : (
                    <span className="rounded-full bg-emerald-500/15 px-2 py-0.5 text-[10px] text-emerald-600 dark:text-emerald-400">
                      live ai-catalog.json
                    </span>
                  )}
                  {card.track === 'background' && (
                    <button
                      type="button"
                      onClick={() => card.traceId && setOpenTraceId(card.traceId)}
                      className="inline-flex items-center gap-1 rounded-full bg-violet-500/15 px-2 py-0.5 text-[10px] text-violet-600 dark:text-violet-300"
                      title={card.intent ?? undefined}
                    >
                      <BrainCircuit className="size-3" />
                      background · {card.intent ?? 'inferred intent'}
                    </button>
                  )}
                  {card.requiresAuth && (
                    <span className="inline-flex items-center gap-1 rounded-full bg-amber-500/15 px-2 py-0.5 text-[10px] text-amber-600 dark:text-amber-400">
                      <ShieldCheck className="size-3" />
                      {card.authKind === 'oauth' ? 'OAuth' : 'auth required'}
                    </span>
                  )}
                </div>

                <div className="mt-0.5 flex items-baseline gap-2">
                  <h4 className="truncate text-sm font-semibold text-text-primary">{card.title}</h4>
                  {card.version && (
                    <span className="text-[11px] text-text-secondary">v{card.version}</span>
                  )}
                </div>

                <p className="mt-0.5 text-xs text-text-secondary line-clamp-2">
                  {card.description}
                </p>
                <p className="mt-1 text-[11px] text-text-secondary">
                  source: <span className="font-medium">{card.sourceDomain}</span>
                  {card.endpoint ? ` · ${card.endpoint}` : ''}
                </p>

                {card.kind === 'skills' && (card.skills ?? []).length > 0 && (
                  <ul className="mt-1.5 flex flex-wrap gap-1.5">
                    {(card.skills ?? []).map((s) => (
                      <li
                        key={s.name}
                        className="rounded-md bg-background-muted px-2 py-0.5 text-[11px] text-text-secondary"
                        title={s.description}
                      >
                        {s.name}
                      </li>
                    ))}
                  </ul>
                )}

                {/* Auth token entry */}
                {st === 'auth' && (
                  <div className="mt-2">
                    {card.authInstructions && (
                      <p className="mb-1 text-[11px] text-amber-600 dark:text-amber-400">
                        {card.authInstructions}
                      </p>
                    )}
                    <input
                      type="password"
                      autoFocus
                      placeholder="Paste auth token / API key"
                      value={tokens[card.id] || ''}
                      onChange={(e) => setTokens((p) => ({ ...p, [card.id]: e.target.value }))}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') install(card);
                      }}
                      className="w-full rounded-md border border-borderSubtle bg-background-primary px-2 py-1 text-xs text-text-primary outline-none focus:border-borderStandard"
                    />
                  </div>
                )}

                {/* Result / error */}
                {st === 'done' && (
                  <div className="mt-2 rounded-md bg-green-500/10 px-2 py-1.5">
                    <p className="flex items-center gap-1 text-[11px] font-medium text-green-700 dark:text-green-400">
                      <Check className="size-3" />{' '}
                      {(rowState[card.id] as { message: string }).message}
                    </p>
                    {(rowState[card.id] as { tools: string[] }).tools.length > 0 && (
                      <p className="mt-0.5 truncate text-[11px] text-text-secondary">
                        tools: {(rowState[card.id] as { tools: string[] }).tools.join(', ')}
                      </p>
                    )}
                  </div>
                )}
                {st === 'error' && (
                  <p className="mt-2 text-[11px] text-red-600 dark:text-red-400">
                    {(rowState[card.id] as { message: string }).message}
                  </p>
                )}
              </div>

              {/* Actions */}
              {st !== 'done' && (
                <div className="flex shrink-0 items-center gap-1.5">
                  {st !== 'installing' && (
                    <Button
                      variant="ghost"
                      size="xs"
                      onClick={() => dismiss(card)}
                      aria-label="Dismiss"
                    >
                      <X className="size-3.5" />
                    </Button>
                  )}
                  <Button
                    variant="default"
                    size="sm"
                    disabled={st === 'installing'}
                    onClick={() => install(card)}
                  >
                    {st === 'installing' ? (
                      <>
                        <Loader2 className="size-3.5 animate-spin" /> Installing
                      </>
                    ) : isServer ? (
                      st === 'auth' ? (
                        'Connect'
                      ) : (
                        'Install'
                      )
                    ) : (
                      'View'
                    )}
                  </Button>
                </div>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
