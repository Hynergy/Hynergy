package dev.hynergy.core.port;

import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;

/**
 * Exact world-oriented pair geometry supplied to a narrow-phase resolver.
 *
 * <p>{@code ownerD*} is the second owner position relative to the first.
 * {@code anchorD*} is the second transformed anchor relative to the first.
 * The first/second ordering is the registered resolver ordering, which can be
 * opposite to the queried source/target ordering for reverse adapter discovery.</p>
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
