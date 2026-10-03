package dev.hynergy.core.port;

import java.util.Objects;

/**
 * Identifies a type of port interaction and its result type.
 *
 * @param <R> result type for the domain
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
