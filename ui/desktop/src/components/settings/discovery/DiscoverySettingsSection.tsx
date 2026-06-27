import { useEffect, useState } from 'react';
import { Check } from 'lucide-react';
import { useConfig } from '../../ConfigContext';
import { cn } from '../../../utils';

const CONFIG_KEY = 'discovery_mode';

type Mode = 'off' | 'light' | 'full';

const OPTIONS: { value: Mode; title: string; description: string }[] = [
  {
    value: 'off',
    title: 'Off',
    description: 'No background discovery. Goose never probes the domains it touches.',
  },
  {
    value: 'light',
    title: 'Light-touch',
    description:
      'Only probe domains that appear in fetch / search / browse tool calls. Conservative — recommended default.',
  },
  {
    value: 'full',
    title: 'Full',
    description:
      'Probe every domain that appears in any tool call (broadest egress capture).',
  },
];

/**
 * Settings control for the AI Catalog / Server Card auto-discovery sidecar.
 * Persists `discovery_mode` to config; new sessions pick it up at agent start.
 */
export function DiscoverySettingsSection() {
  const { read, upsert } = useConfig();
  const [mode, setMode] = useState<Mode>('light');
  const [saving, setSaving] = useState<Mode | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const v = (await read(CONFIG_KEY, false)) as string | undefined;
        if (v === 'off' || v === 'light' || v === 'full') setMode(v);
      } catch {
        /* default light */
      }
    })();
  }, [read]);

  const choose = async (value: Mode) => {
    setMode(value);
    setSaving(value);
    try {
      await upsert(CONFIG_KEY, value, false);
    } catch {
      /* ignore */
    } finally {
      setSaving(null);
    }
  };

  return (
    <div className="flex flex-col gap-1" data-testid="discovery-settings">
      <div className="px-2 pt-1">
        <h3 className="text-text-primary">Server Card discovery</h3>
        <p className="text-xs text-text-secondary max-w-md mt-[2px]">
          Watch the domains this session touches and surface installable MCP servers and Skills
          bundles they advertise (via <code>/.well-known/ai-catalog.json</code>). Applies to new
          sessions.
        </p>
      </div>
      <div className="flex flex-col">
        {OPTIONS.map((opt) => {
          const selected = mode === opt.value;
          return (
            <button
              key={opt.value}
              type="button"
              onClick={() => choose(opt.value)}
              className={cn(
                'flex items-start justify-between gap-3 py-2 px-2 rounded-lg text-left transition-all',
                'hover:bg-background-secondary',
                selected && 'bg-background-secondary'
              )}
            >
              <div>
                <span className="text-text-primary text-sm">{opt.title}</span>
                <p className="text-xs text-text-secondary max-w-md mt-[2px]">{opt.description}</p>
              </div>
              <span className="mt-1 shrink-0">
                {selected ? (
                  <Check className="size-4 text-text-primary" />
                ) : saving === opt.value ? (
                  <span className="text-[10px] text-text-secondary">…</span>
                ) : null}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
