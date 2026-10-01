package dev.hynergy.core.port;

import java.util.Objects;

/**
 * Immutable asset/runtime definition of one logical block port.
 */
public record PortDefinition<P, R>(
        int localId,
        PortOffset anchor,
        PortReach reach,
        PortStandard<P, R> standard,
        P profile
) {
    public PortDefinition(
            int localId,
            PortOffset anchor,
            PortReach reach,
            PortStandard<P, R> standard,
            P profile
    ) {
        if (localId < 0) {
            throw new IllegalArgumentException("localId must be non-negative");
        }
        this.localId = localId;
        this.anchor = Objects.requireNonNull(anchor, "anchor");
        this.reach = Objects.requireNonNull(reach, "reach");
        this.standard = Objects.requireNonNull(standard, "standard");
        if (!standard.profileType().isInstance(profile)) {
            throw new IllegalArgumentException(
                    "Profile for standard " + standard.id() + " must be a " + standard.profileType().getName()
            );
        }
        this.profile = profile;
    }
}
