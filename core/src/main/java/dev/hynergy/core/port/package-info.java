/**
 * Discovery-only block interaction ports.
 *
 * <p>This package answers which currently exposed endpoints can interact and
 * what a domain-specific resolver says that interaction means. It deliberately
 * does not own connection lifetime: callers choose whether to use results once,
 * cache neighbours, or maintain their own topology.</p>
 */
package dev.hynergy.core.port;
