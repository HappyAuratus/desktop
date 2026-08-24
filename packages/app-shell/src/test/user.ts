import userEvent from "@testing-library/user-event";

/**
 * Creates a user-event instance with faithful real-input scheduling.
 *
 * This is the default for tests: `delay: 0` keeps a real task gap between
 * key/pointer events, which UI components rely on (Base UI popups render
 * between pointerdown and pointerup; ProseMirror reconciles selection moves
 * between keystrokes). Real users always produce those gaps, so tests must
 * too. Always create instances through this helper (enforced by an ESLint
 * rule) — see `setupTypingUser` for the typing-heavy exception.
 *
 * `delay: 0` schedules one real `setTimeout` per event, so typing speed
 * depends on event-loop contention: under parallel workers the timers starve
 * and typing-heavy tests intermittently exceed the 5s test timeout. Worse,
 * vitest does not cancel an awaited-but-timed-out `user.keyboard()` chain —
 * the surviving keystrokes keep firing after the test dies, each one resolved
 * against the *current* `document.activeElement`, so they end up typed into
 * the next test's focused editor: corrupted assertions plus act() warnings
 * that fail the clean-stderr gate. For tests that type hundreds of
 * characters into the ProseMirror composer, use `setupTypingUser` instead.
 */
export function setupUser(options: Parameters<typeof userEvent.setup>[0] = {}) {
  return userEvent.setup({ ...options, delay: options.delay ?? 0 });
}

/**
 * Creates a user-event instance that dispatches keyboard events without real
 * per-keystroke timers — the tool for typing-heavy ProseMirror composer
 * tests.
 *
 * `delay: null` removes the per-keystroke `setTimeout`, so a typing-heavy
 * test no longer amplifies event-loop contention, and ~200-keystroke tests
 * run ~7x faster.
 *
 * Not timer-proof: every keystroke dispatched through React's `act` crosses
 * the React scheduler's MessageChannel (a macrotask), so the event loop
 * still turns between keystrokes. Under total CPU starvation a pending
 * test-timeout timer can therefore fire mid-chain and turn the chain into a
 * zombie — the queue-level mitigations (worker cap, 20s test timeout) live
 * in vitest.config.ts.
 *
 * Caveats — zero-gap input is physically impossible for real users, and two
 * behaviors depend on real task gaps:
 * - Pointer: Base UI popups need to render between pointerdown and
 *   pointerup; use `setupUser` for anything that clicks menus/selects.
 * - Keyboard: ProseMirror does not collapse a non-empty selection on arrow
 *   keys (prosemirror-view `selectHorizontally` declines, expecting the
 *   browser to do it); user-event then collapses the DOM selection as its
 *   default keydown behavior, and PM resyncs its internal selection from
 *   the DOM only when jsdom's `selectionchange` task fires. A zero-gap
 *   chain outruns that task, so the next keypress inserts at the stale
 *   selection and replaces the selected text. Keep the navigation key and
 *   the text in separate `keyboard()` calls — the await between calls is a
 *   task boundary, exactly like real typing.
 */
export function setupTypingUser() {
  return setupUser({ delay: null });
}
