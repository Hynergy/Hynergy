package dev.hynergy.core.port;

import java.util.Objects;

/**
 * Identifies one connection protocol within a {@link PortDomain}.
 *
 * <p>A logical port exposes exactly one standard. Different standards can
 * interoperate only through an adapter registered during setup.</p>
 *
 * @param <P> typed profile supplied by ports of this standard
 * @param <R> domain resolution type
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
