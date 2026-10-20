import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { DiscoverySettingsSection } from './DiscoverySettingsSection';

const read = vi.fn();
const upsert = vi.fn();
vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({ read, upsert }),
}));

beforeEach(() => {
  read.mockReset();
  upsert.mockReset();
});

describe('DiscoverySettingsSection', () => {
  it('reflects the persisted mode and writes the chosen mode back', async () => {
    read.mockResolvedValue('light');
    upsert.mockResolvedValue(undefined);

    render(<DiscoverySettingsSection />);

    // All three modes render.
    expect(screen.getByText('Off')).toBeInTheDocument();
    expect(screen.getByText('Light-touch')).toBeInTheDocument();
    expect(screen.getByText('Full')).toBeInTheDocument();

    // Reads the current value under the documented config key.
    await waitFor(() => expect(read).toHaveBeenCalledWith('discovery_mode', false));

    // Choosing "Full" persists discovery_mode=full (non-secret).
    fireEvent.click(screen.getByText('Full'));
    await waitFor(() => expect(upsert).toHaveBeenCalledWith('discovery_mode', 'full', false));

    // Choosing "Off" persists discovery_mode=off.
    fireEvent.click(screen.getByText('Off'));
    await waitFor(() => expect(upsert).toHaveBeenCalledWith('discovery_mode', 'off', false));
  });
});
