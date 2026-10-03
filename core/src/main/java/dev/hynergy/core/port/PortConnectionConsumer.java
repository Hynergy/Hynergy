package dev.hynergy.core.port;

/**
 * Receives a result for a compatible port pair during discovery.
 *
 * <p>The target coordinates and port ID identify the other port.
 * {@code sourceIsResolverFirst} is true when the source port is the resolver's
 * first argument.</p>
 *
 * @param <R> result type for the domain
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
