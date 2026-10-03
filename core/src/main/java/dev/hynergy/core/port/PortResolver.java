package dev.hynergy.core.port;

import org.jspecify.annotations.Nullable;

/**
 * Evaluates one candidate port pair.
 *
 * <p>Return {@code null} to reject the pair. Otherwise, return a result for the
 * consumer. The resolver should not change connection state.</p>
 */
@FunctionalInterface
public interface PortResolver<A, B, R> {
    @Nullable R resolve(A firstProfile, B secondProfile, PortGeometry geometry);
}
