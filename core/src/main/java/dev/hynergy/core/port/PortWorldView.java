package dev.hynergy.core.port;

import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import org.jspecify.annotations.Nullable;

/**
 * Provides port layouts and block rotations for world positions.
 *
 * <p>Discovery requests a rotation only when {@link #portsAt(int, int, int)}
 * returns a layout.</p>
 */
public interface PortWorldView {
    /**
     * Returns the port layout selected for the position.
     *
     * @return the layout, or {@code null} if the block has no ports or the position is unavailable
     */
    @Nullable BlockPortDefinition portsAt(int x, int y, int z);

    RotationTuple rotation(int x, int y, int z);
}