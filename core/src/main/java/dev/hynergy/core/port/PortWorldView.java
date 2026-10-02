package dev.hynergy.core.port;

import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;

public interface PortWorldView {
    int blockTypeId(int x, int y, int z);

    RotationTuple rotation(int x, int y, int z);
}
