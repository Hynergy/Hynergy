package dev.hynergy.core.port;

import java.util.Objects;

/**
 * Identifies a port standard within a {@link PortDomain}.
 *
 * <p>Each port has one standard. Ports with different standards require a
 * registered adapter to interact.</p>
 *
 * @param <P> profile type required by the standard
 * @param <R> result type for the domain
 */
public final class PortStandard<P, R> {
    private final String id;
    private final PortDomain<R> domain;
    private final Class<P> profileType;

    PortStandard(String id, PortDomain<R> domain, Class<P> profileType) {
        this.id = Objects.requireNonNull(id, "id");
        this.domain = Objects.requireNonNull(domain, "domain");
        this.profileType = Objects.requireNonNull(profileType, "profileType");
    }

    public String id() {
        return id;
    }

    public PortDomain<R> domain() {
        return domain;
    }

    public Class<P> profileType() {
        return profileType;
    }
}
