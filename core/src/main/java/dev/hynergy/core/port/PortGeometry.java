package dev.hynergy.core.port;

import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;

/**
 * Describes a port pair in world coordinates.
 *
 * <p>{@code ownerDx}, {@code ownerDy}, and {@code ownerDz} locate the second block
 * relative to the first block. {@code anchorDx}, {@code anchorDy}, and
 * {@code anchorDz} locate the second rotated anchor relative to the first
 * rotated anchor.</p>
 *
 * <p>The first and second ports follow the resolver argument order. This order
 * can differ from the discovery source and target order.</p>
 */
public record PortGeometry(
        int ownerDx,
        int ownerDy,
        int ownerDz,
        int anchorDx,
        int anchorDy,
        int anchorDz,
        RotationTuple firstRotation,
        RotationTuple secondRotation
) {
}
