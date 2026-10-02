package dev.hynergy.core.port;

import org.jspecify.annotations.Nullable;

/**
 * Performs domain-specific narrow-phase resolution for one candidate pair.
 *
 * <p>Resolvers should be side-effect free with respect to connection ownership.
 * Returning {@code null} rejects only the current candidate. A non-null result is
 * snapshot information returned to the discovery caller; the Port API does not
 * retain it.</p>
 */
@FunctionalInterface
public interface PortResolver<A, B, R> {
    @Nullable R resolve(A firstProfile, B secondProfile, PortGeometry geometry);
}
