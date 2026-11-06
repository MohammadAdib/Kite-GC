<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Marc Hoffmann (b14ckyy)
-->

<!-- Flight Mode widget — shows current flight mode as colored badge. On a MAVLink (ArduPilot / PX4)
     link a click opens a mode picker: the same list, RC lock and mode switch as the Vehicle Control
     panel's mode buttons. -->
<script lang="ts">
  import { t } from 'svelte-i18n';
  import type { TelemetryData } from "$lib/stores/telemetry";
  import { fcAvailableModes } from "$lib/stores/telemetry";
  import { modeLabel, modeColor, modeShort, modeCategory } from "$lib/helpers/flightModeRegistry";
  import { mission, replayActive } from "$lib/stores/mission";
  import { arduMission, arduVehicleClass } from "$lib/stores/missionArdupilot";
  import { autopilotSystem } from "$lib/stores/autopilotContext";
  import { replayWpTotal } from "$lib/stores/navStatus";
  import { availableModesFor, type MavMode } from "$lib/helpers/mavModes";
  import { controlAvailable, stickModesUnlocked, activeMode, setMode } from "$lib/controllers/vehicleControl";
  import { contextMenu, openContextMenu, type ContextMenuItem } from "$lib/stores/contextMenu";

  let { telem, size = 9, editing = false }: { telem: TelemetryData; size?: number; editing?: boolean } = $props();

  // Canonical, protocol-agnostic flight mode (see flightModeRegistry).
  let primary = $derived(telem.flightMode.primary);
  let label = $derived(modeLabel(primary));
  let color = $derived(modeColor(primary));
  let modifiers = $derived(telem.flightMode.modifiers);

  // Badge text size from the label, so short modes read big and long ones still fit the card. A
  // multi-word label may wrap onto two lines, so it is sized by its longest word.
  let badgeK = $derived.by(() => {
    const words = label.split(' ');
    const longest = Math.max(1, ...words.map((w) => w.length));
    const k = Math.min(0.22, 1.15 / longest);
    return words.length > 1 ? Math.min(k, 0.19) : k;
  });

  // In a MISSION-category mode, show the FC's current target waypoint as "WP N/X"
  // (N = active waypoint number, X = total waypoints). Only when an active WP number is known.
  let inMission = $derived(modeCategory(primary) === 'mission');
  // X (total): in replay use the flown mission's count (linked library mission or Blackbox
  // header); live uses the loaded planner mission's length.
  // X (total): replay → flown mission count; live → the loaded planner mission length, from the
  // INAV store on INAV, otherwise the ArduPilot store (which PX4 shares — 'px4' is its own
  // system value, so discriminate on 'inav', never on 'ardupilot').
  let wpTotal = $derived(
    $replayActive ? ($replayWpTotal ?? 0)
    : $autopilotSystem === 'inav' ? $mission.waypoints.length
    : $arduMission.length,
  );
  let wpText = $derived.by(() => {
    if (!inMission) return null;
    const n = telem.activeWpNumber;
    if (n <= 0) return null;
    return wpTotal > 0 ? `WP ${n}/${wpTotal}` : `WP ${n}`;
  });

  // Mode picker: only on a live MAVLink link (INAV modes are switched on the transmitter), never while
  // a log replays (the badge then shows the replay, not the connected FC), and not while the dock is
  // being edited (a long press there picks the widget up — its release must not open the menu).
  let canPick = $derived($controlAvailable && !$replayActive && !editing);

  // A second click on the card closes its picker rather than reopening it. The app-wide menu host
  // closes any open menu on pointerdown (window, after this card's own handler), so note here whether
  // the open menu was ours before that happens.
  let pickerItems: ContextMenuItem[] | null = null;
  let pickerWasOpen = false;
  function notePickerState() {
    pickerWasOpen = $contextMenu.open && $contextMenu.items === pickerItems;
  }

  function openPicker(e: MouseEvent) {
    if (!canPick) return;
    if (pickerWasOpen) { pickerWasOpen = false; return; }
    const modes = availableModesFor($autopilotSystem, $arduVehicleClass, $fcAvailableModes);
    const activeKey = $activeMode?.key;
    const unlocked = $stickModesUnlocked;
    // Non-breaking space keeps every label aligned with the active mode's check mark.
    const entry = (m: MavMode, locked: boolean): ContextMenuItem => ({
      label: m.name,
      icon: m.key === activeKey ? '✓' : ' ',
      disabled: locked,
      action: m.key === activeKey ? undefined : () => { void setMode(m); },
    });

    const items: ContextMenuItem[] = modes.filter((m) => !m.stick).map((m) => entry(m, false));
    const stick = modes.filter((m) => m.stick);
    if (stick.length) {
      if (items.length) items.push({ separator: true });
      // Stick-flown modes: listed, but locked without an RC source — same interlock as the panel.
      if (!unlocked) items.push({ label: $t('control.noRcLink'), icon: ' ', disabled: true });
      for (const m of stick) items.push(entry(m, !unlocked));
    }

    // Drop down from the card; a bottom-docked card gets it above instead (anchorTop).
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    pickerItems = items;
    openContextMenu(r.left, r.bottom + 4, items, r.top - 4);
  }
</script>

<button
  type="button"
  class="widget-card"
  class:pickable={canPick}
  style="--ws: {size}px"
  onpointerdown={notePickerState}
  onclick={openPicker}
  aria-haspopup={canPick ? 'menu' : undefined}
  aria-disabled={!canPick}
  title={canPick ? $t('control.mode.select') : undefined}
>
  <span class="w-label">MODE{#if canPick}<span class="w-caret" aria-hidden="true">▾</span>{/if}</span>
  <span class="w-body">
    <span
      class="w-mode"
      class:wrap={label.includes(' ')}
      style="background: {color}; color: {color === '#c0c0c0' ? '#1a1a1a' : '#fff'}; font-size: calc(var(--ws) * {badgeK});"
    >
      {label}
    </span>
    {#if modifiers.length > 0}
      <span class="w-mods">
        {#each modifiers as mod}
          <span class="w-mod-tag">{modeShort(mod)}</span>
        {/each}
      </span>
    {/if}
  </span>
  {#if wpText}
    <span class="w-wp">{wpText}</span>
  {/if}
</button>

<style>
  .widget-card {
    width: var(--ws);
    height: var(--ws);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-start;
    background: rgba(30, 30, 30, 0.75);
    backdrop-filter: var(--glass-blur, blur(6px));
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: calc(var(--ws) * 0.08);
    gap: calc(var(--ws) * 0.03);
    box-sizing: border-box;
    padding: calc(var(--ws) * 0.05) calc(var(--ws) * 0.05) calc(var(--ws) * 0.04);
    /* <button> reset — it is a card, not a form control. */
    margin: 0;
    font: inherit;
    color: inherit;
    text-align: center;
    cursor: default;
    -webkit-tap-highlight-color: transparent;
  }
  .widget-card.pickable {
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
  }
  .widget-card.pickable:hover {
    border-color: rgba(255, 145, 27, 0.45);
    background: rgba(40, 40, 40, 0.82);
  }
  .widget-card.pickable:focus-visible {
    outline: 2px solid rgba(255, 145, 27, 0.7);
    outline-offset: 1px;
  }
  .w-label {
    display: inline-flex;
    align-items: center;
    gap: calc(var(--ws) * 0.03);
    font-size: calc(var(--ws) * 0.13);
    font-weight: 600;
    color: #ff911b;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .w-caret {
    font-size: calc(var(--ws) * 0.11);
    line-height: 1;
    opacity: 0.85;
  }
  /* The badge (and any modifier chips) centred in the space under the label, instead of hugging the
     top and leaving the lower half of the card empty. */
  .w-body {
    flex: 1;
    min-height: 0;
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: calc(var(--ws) * 0.04);
  }
  .w-mode {
    max-width: 100%;
    box-sizing: border-box;
    font-weight: 700;
    line-height: 1.15;
    padding: calc(var(--ws) * 0.03) calc(var(--ws) * 0.05);
    border-radius: calc(var(--ws) * 0.04);
    text-transform: uppercase;
    letter-spacing: 0.03em;
    white-space: nowrap;
  }
  .w-mode.wrap {
    white-space: normal;
  }
  .w-mods {
    display: flex;
    gap: calc(var(--ws) * 0.02);
    flex-wrap: wrap;
    justify-content: center;
  }
  .w-mod-tag {
    font-size: calc(var(--ws) * 0.09);
    font-weight: 600;
    color: #949494;
    background: rgba(255, 255, 255, 0.08);
    padding: calc(var(--ws) * 0.01) calc(var(--ws) * 0.03);
    border-radius: calc(var(--ws) * 0.02);
  }
  .w-wp {
    margin-bottom: calc(var(--ws) * 0.02);
    font-size: calc(var(--ws) * 0.15);
    font-weight: 700;
    color: #ff911b;
    letter-spacing: 0.04em;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
