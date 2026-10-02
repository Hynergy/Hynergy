package dev.hynergy.core.electricity.wires;

import com.hypixel.hytale.component.ComponentType;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import dev.hynergy.core.electricity.ElectricalPortConnection;
import dev.hynergy.core.electricity.ElectricalPortProfile;
import dev.hynergy.core.port.BlockPortDefinition;
import dev.hynergy.core.port.PortModule;
import dev.hynergy.core.port.PortStandard;
import it.unimi.dsi.fastutil.ints.IntIterator;
import it.unimi.dsi.fastutil.ints.IntOpenHashSet;

import java.util.IdentityHashMap;
import java.util.Map;
import java.util.Objects;

public final class WireBlockPortDefinitions {
    private final PortModule portModule;
    private final PortStandard<ElectricalPortProfile, ElectricalPortConnection> conductorStandard;
    private final ComponentType<ChunkStore, WireComponent> wireComponentType;

    private final Map<WireConfig, BlockPortDefinition> compiledDefinitions = new IdentityHashMap<>();
    private final IntOpenHashSet publishedBlockTypes = new IntOpenHashSet();
    private final IntOpenHashSet rebuiltBlockTypes = new IntOpenHashSet();

    public WireBlockPortDefinitions(
            PortModule portModule,
            PortStandard<
                    ElectricalPortProfile,
                    ElectricalPortConnection
                    > conductorStandard,
            ComponentType<ChunkStore, WireComponent> wireComponentType
    ) {
        this.portModule =
                Objects.requireNonNull(portModule, "portModule");

        this.conductorStandard =
                Objects.requireNonNull(
                        conductorStandard,
                        "conductorStandard"
                );

        this.wireComponentType =
                Objects.requireNonNull(
                        wireComponentType,
                        "wireComponentType"
                );
    }

    public synchronized void rebuild() {
        compiledDefinitions.clear();
        rebuiltBlockTypes.clear();

        var blockTypes = BlockType.getAssetMap();
        var wireConfigs = WireConfig.getAssetMap();

        for (BlockType blockType : blockTypes.getAssetMap().values()) {
            var blockEntity = blockType.getBlockEntity();
            if (blockEntity == null) {
                continue;
            }

            WireComponent wireComponent =
                    blockEntity.getComponent(wireComponentType);

            if (wireComponent == null) {
                continue;
            }

            String configId = wireComponent.getConfigId();
            if (configId == null) {
                continue;
            }

            WireConfig config = wireConfigs.getAsset(configId);
            if (config == null) {
                continue;
            }

            int blockTypeIndex =
                    blockTypes.getIndex(blockType.getId());

            if (blockTypeIndex < 0) {
                throw new IllegalStateException(
                        "Loaded block type has no runtime index: "
                                + blockType.getId()
                );
            }

            BlockPortDefinition definition =
                    compiledDefinitions.get(config);

            if (definition == null) {
                definition =
                        config.buildPortDefinition(conductorStandard);

                compiledDefinitions.put(config, definition);
            }

            portModule.setBlockPorts(
                    blockTypeIndex,
                    definition
            );

            rebuiltBlockTypes.add(blockTypeIndex);
        }

        IntIterator iterator = publishedBlockTypes.iterator();
        while (iterator.hasNext()) {
            int blockTypeIndex = iterator.nextInt();

            if (!rebuiltBlockTypes.contains(blockTypeIndex)) {
                portModule.clearBlockPorts(blockTypeIndex);
            }
        }

        publishedBlockTypes.clear();
        publishedBlockTypes.addAll(rebuiltBlockTypes);
    }
}