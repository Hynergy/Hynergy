package dev.hynergy.core.port;

import java.util.Objects;

/**
 * Defines one port in a block layout.
 *
 * <p>The anchor and reach use the block's local coordinates. The profile supplies
 * the data required by the port standard.</p>
 *
 * @param <P> profile type required by the standard
 * @param <R> result type for the domain
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
