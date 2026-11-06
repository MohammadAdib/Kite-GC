// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// Global custom context-menu state. A single <ContextMenu> mounted at the app
// root renders whatever the active opener requested; `use:contextMenu` (action)
// or openContextMenu() drives it. Replaces the native WebView menu.

import { writable } from 'svelte/store';

export interface ContextMenuItem {
  label?: string;
  /** Run on click (ignored for separators / submenu parents). */
  action?: () => void;
  /** Optional leading glyph. */
  icon?: string;
  disabled?: boolean;
  /** Red styling for destructive actions. */
  danger?: boolean;
  /** Render a divider instead of an item. */
  separator?: boolean;
  /** Nested items — opens a flyout to the side. */
  submenu?: ContextMenuItem[];
}

interface ContextMenuState {
  open: boolean;
  x: number;
  y: number;
  items: ContextMenuItem[];
  /** Dropdown use: the opener's top edge. When the menu doesn't fit below `y`, it is placed with its
   *  bottom edge here (above the opener) instead of being pushed up over it. */
  anchorTop?: number;
}

export const contextMenu = writable<ContextMenuState>({ open: false, x: 0, y: 0, items: [] });

export function openContextMenu(x: number, y: number, items: ContextMenuItem[], anchorTop?: number): void {
  if (!items?.length) return;
  contextMenu.set({ open: true, x, y, items, anchorTop });
}

export function closeContextMenu(): void {
  contextMenu.update((s) => (s.open ? { ...s, open: false } : s));
}
