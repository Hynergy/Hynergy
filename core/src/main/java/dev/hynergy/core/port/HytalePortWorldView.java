package dev.hynergy.core.port;

import com.hypixel.hytale.component.Ref;
import com.hypixel.hytale.math.util.ChunkUtil;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import com.hypixel.hytale.server.core.universe.world.World;
import com.hypixel.hytale.server.core.universe.world.chunk.section.BlockSection;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import org.jspecify.annotations.Nullable;

import java.util.Objects;

/**
 * Reads port-discovery block data from already-loaded Hytale chunk sections.
 *
 * <p>This view never loads or generates chunks. Positions whose section is not
 * currently available are reported as unavailable through a negative block ID.</p>
 */
public final class HytalePortWorldView implements PortWorldView {
    private final ChunkStore chunkStore;

    public HytalePortWorldView(World world) {
        this(Objects.requireNonNull(world, "world").getChunkStore());
    }

    HytalePortWorldView(ChunkStore chunkStore) {
        this.chunkStore = Objects.requireNonNull(chunkStore, "chunkStore");
    }

    @Override
    public int blockTypeId(int x, int y, int z) {
        return blockTypeIdFromSection(
            loadedSection(x, y, z),
            localCoordinate(x),
            localCoordinate(y),
            localCoordinate(z)
        );
    }

    @Override
    public RotationTuple rotation(int x, int y, int z) {
        BlockSection section = loadedSection(x, y, z);
        if (section == null) {
            throw new IllegalStateException("Block section became unavailable during port discovery");
        }

        return section.getRotation(localCoordinate(x), localCoordinate(y), localCoordinate(z));
    }

    private @Nullable BlockSection loadedSection(int x, int y, int z) {
        Ref<ChunkStore> sectionReference = chunkStore.getChunkSectionReferenceAtBlock(x, y, z);
        if (sectionReference == null) {
            return null;
        }

        return chunkStore.getStore().getComponent(sectionReference, BlockSection.getComponentType());
    }

    static int localCoordinate(int coordinate) {
        return ChunkUtil.localCoordinate(coordinate);
    }

    static int blockTypeIdFromSection(@Nullable BlockSection section, int localX, int localY, int localZ) {
        return section == null ? -1 : section.get(localX, localY, localZ);
    }
}
