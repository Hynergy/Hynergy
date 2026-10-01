package dev.hynergy.core.port;

/**
 * Receives one relationship discovered during the current query.
 *
 * <p>Callbacks are snapshots. The Port API does not retain or later invalidate
 * them; callers own any caching, connection state, removal, or revalidation they
 * derive from these values.</p>
 */
@FunctionalInterface
public interface PortConnectionConsumer<R> {
    void accept(
        int targetX,
        int targetY,
        int targetZ,
        int targetPortId,
        R resolution,
        boolean sourceIsResolverFirst
    );
}
