import type { DiscoveryTraceDto } from '../../acp/discovery';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '../ui/dialog';
import { cn } from '../../utils';

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <div className="text-[11px] font-semibold uppercase tracking-wide text-text-secondary">
        {label}
      </div>
      {children}
    </div>
  );
}

function Pre({ children }: { children: React.ReactNode }) {
  return (
    <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-background-secondary px-3 py-2 font-mono text-[11px] text-text-primary">
      {children}
    </pre>
  );
}

export default function DiscoveryTraceDialog({
  trace,
  onClose,
}: {
  trace: DiscoveryTraceDto | null;
  onClose: () => void;
}) {
  return (
    <Dialog open={trace !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="max-w-2xl max-h-[85vh] overflow-y-auto"
        data-testid="discovery-trace"
      >
        {trace && (
          <>
            <DialogHeader>
              <DialogTitle>Background discovery trace</DialogTitle>
              <DialogDescription>
                {trace.status} · {trace.model} · started{' '}
                {new Date(trace.startedAt).toLocaleTimeString()}
              </DialogDescription>
            </DialogHeader>
            <div className="flex flex-col gap-4 text-sm">
              <Section label="User requests considered">
                <ol className="list-decimal pl-5 text-text-primary">
                  {trace.requests.map((r, i) => (
                    <li key={i}>{r}</li>
                  ))}
                </ol>
              </Section>
              <Section label="Reasoner prompt (system + user)">
                <Pre>{`${trace.systemPrompt}\n\n---\n${trace.prompt}`}</Pre>
              </Section>
              <Section label="Reasoner output">
                <Pre>{trace.rawOutput || '(waiting for the reasoner…)'}</Pre>
              </Section>
              <Section label="Inferred intent">
                <div className="text-text-primary">{trace.intent ?? '—'}</div>
                {trace.rationale && (
                  <div className="text-xs text-text-secondary">{trace.rationale}</div>
                )}
              </Section>
              <Section label="Candidate domains">
                <div className="flex flex-wrap gap-1">
                  {trace.candidateDomains.length === 0 ? (
                    <span className="text-text-secondary">none</span>
                  ) : (
                    trace.candidateDomains.map((d) => (
                      <code
                        key={d}
                        className="rounded bg-background-secondary px-1.5 py-0.5 text-xs"
                      >
                        {d}
                      </code>
                    ))
                  )}
                </div>
              </Section>
              <Section label=".well-known probes">
                {trace.probes.length === 0 ? (
                  <span className="text-text-secondary">none</span>
                ) : (
                  <ul className="flex flex-col gap-1">
                    {trace.probes.map((p) => (
                      <li key={p.url} className="text-xs">
                        <code className="text-text-primary">GET {p.url}</code>
                        <span
                          className={cn(
                            'ml-2',
                            (p.found ?? []).length ? 'text-emerald-500' : 'text-text-secondary'
                          )}
                        >
                          → {p.outcome}
                        </span>
                        {(p.found ?? []).map((f) => (
                          <div key={f} className="pl-4 text-text-primary">
                            • {f}
                          </div>
                        ))}
                      </li>
                    ))}
                  </ul>
                )}
              </Section>
              <Section label="Decision">
                <div className="text-text-primary">{trace.decision}</div>
              </Section>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
