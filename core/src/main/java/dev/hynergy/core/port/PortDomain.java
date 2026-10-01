package dev.hynergy.core.port;

import java.util.Objects;

/**
 * Identifies one independent kind of port interaction and its resolution type.
 *
 * <p>Domains define semantics, not connection lifetime. A discovered resolution
 * belongs to the caller that requested discovery.</p>
 *
 * @param <R> result produced when two ports in this domain resolve
 */
public final class PortDomain<R> {
    private final String id;

    PortDomain(String id) {
        this.id = Objects.requireNonNull(id, "id");
    }

    public String id() {
        return id;
    }
}
