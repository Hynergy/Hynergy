package dev.hynergy.core.port;

import com.hypixel.hytale.component.Ref;
import com.hypixel.hytale.math.util.ChunkUtil;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import com.hypixel.hytale.server.core.universe.world.World;
import com.hypixel.hytale.server.core.universe.world.chunk.section.BlockSection;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import org.jspecify.annotations.Nullable;

import java.util.Objects;
import java.util.function.IntFunction;

/**
 * Selects port layouts from block types in loaded Hytale chunk sections.
 *
 * <p>The supplied lookup maps each block type ID to a port layout. This view
 * does not load or generate chunks. A position has no discoverable ports if
 * its section is unavailable.</p>
 */
public final class HytalePortWorldView implements PortWorldView {
    private final ChunkStore chunkStore;
    private final IntFunction<@Nullable BlockPortDefinition> blockPorts;

    public HytalePortWorldView(World world, IntFunction<@Nullable BlockPortDefinition> blockPorts) {
        this(Objects.requireNonNull(world, "world").getChunkStore(), blockPorts);
    }

    HytalePortWorldView(ChunkStore chunkStore, IntFunction<@Nullable BlockPortDefinition> blockPorts) {
        this.chunkStore = Objects.requireNonNull(chunkStore, "chunkStore");
        this.blockPorts = Objects.requireNonNull(blockPorts, "blockPorts");
    }

    @Override
    public @Nullable BlockPortDefinition portsAt(int x, int y, int z) {
        int blockTypeId = blockTypeIdFromSection(
                loadedSection(x, y, z),
                localCoordinate(x),
                localCoordinate(y),
                localCoordinate(z)
        );
        return blockTypeId < 0 ? null : blockPorts.apply(blockTypeId);
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
