package dev.hynergy.core.electricity.wire;

import com.hypixel.hytale.component.*;
import com.hypixel.hytale.component.query.Query;
import com.hypixel.hytale.component.system.RefSystem;
import com.hypixel.hytale.math.util.ChunkUtil;
import com.hypixel.hytale.server.core.modules.block.BlockModule;
import com.hypixel.hytale.server.core.universe.world.World;
import com.hypixel.hytale.server.core.universe.world.chunk.section.BlockSection;
import com.hypixel.hytale.server.core.universe.world.chunk.section.ChunkSection;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import dev.hynergy.core.electricity.ElectricalPortConnection;
import dev.hynergy.core.electricity.ElectricalSystemResource;
import dev.hynergy.core.port.*;
import dev.hynergy.electrical.ElectricalRuntime;
import dev.hynergy.electrical.ElectricalSystem;
import dev.hynergy.electrical.Wire;
import dev.hynergy.electrical.WireId;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import org.jspecify.annotations.NonNull;

public final class WireSystem extends RefSystem<ChunkStore> {

    private final ElectricalRuntime runtime;
    private final ResourceType<ChunkStore, ElectricalSystemResource> electricalSystemResourceType;
    private final ComponentType<ChunkStore, WireComponent> wireComponentType;
    private final PortModule portModule;
    private final PortDomain<ElectricalPortConnection> electricalPortDomain;
    private final Query<ChunkStore> query;

    private final ComponentType<ChunkStore, BlockModule.BlockStateInfo> blockStateInfoComponentType =
            BlockModule.BlockStateInfo.getComponentType();

    private final ThreadLocal<LongOpenHashSet> connectionScratch =
            ThreadLocal.withInitial(
                    () -> new LongOpenHashSet(8)
            );

    public WireSystem(
            ElectricalRuntime runtime,
            ResourceType<
                    ChunkStore,
                    ElectricalSystemResource
                    > electricalSystemResourceType,
            ComponentType<
                    ChunkStore,
                    WireComponent
                    > wireComponentType,
            PortModule portModule,
            PortDomain<
                    ElectricalPortConnection
                    > electricalPortDomain
    ) {
        this.runtime = runtime;
        this.electricalSystemResourceType =
                electricalSystemResourceType;
        this.wireComponentType = wireComponentType;
        this.portModule = portModule;
        this.electricalPortDomain = electricalPortDomain;

        this.query = Query.and(
                wireComponentType,
                blockStateInfoComponentType
        );
    }

    @Override
    public void onEntityAdded(
            @NonNull Ref<ChunkStore> ref,
            @NonNull AddReason reason,
            Store<ChunkStore> store,
            CommandBuffer<ChunkStore> commandBuffer
    ) {
        store.assertThread();

        World world = store.getExternalData().getWorld();
        world.debugAssertInTickingThread();

        WireComponent component =
                commandBuffer.getComponent(
                        ref,
                        wireComponentType
                );

        BlockModule.BlockStateInfo blockStateInfo =
                commandBuffer.getComponent(
                        ref,
                        blockStateInfoComponentType
                );

        if (component == null || blockStateInfo == null) {
            throw new IllegalStateException(
                    "ElectricalWireSystem matched an entity without its required components"
            );
        }

        if (component.getWire() != null) {
            throw new IllegalStateException(
                    "WireComponent already has a runtime Wire handle"
            );
        }

        ElectricalSystem system = store.getResource(electricalSystemResourceType).getOrCreate(runtime, world);

        WireId persistedId = component.getWireId();

        if (persistedId != null) {
            component.setWire(
                    system.resolveWire(persistedId)
            );

            return;
        }

        Wire wire = system.createWire();

        component.setWireId(wire.id());
        component.setWire(wire);

        blockStateInfo.markNeedsSaving(commandBuffer);

        connectNewWire(
                wire,
                blockStateInfo,
                store,
                commandBuffer
        );
    }

    @Override
    public void onEntityRemove(
            @NonNull Ref<ChunkStore> ref,
            @NonNull RemoveReason reason,
            Store<ChunkStore> store,
            CommandBuffer<ChunkStore> commandBuffer
    ) {
        store.assertThread();

        World world = store.getExternalData().getWorld();
        world.debugAssertInTickingThread();

        WireComponent component =
                commandBuffer.getComponent(
                        ref,
                        wireComponentType
                );

        if (component == null) {
            throw new IllegalStateException(
                    "ElectricalWireSystem matched an entity without a WireComponent"
            );
        }

        if (reason == RemoveReason.UNLOAD) {
            component.setWire(null);
            return;
        }

        Wire wire = component.getWire();

        if (wire == null) {
            WireId wireId = component.getWireId();

            if (wireId != null) {
                ElectricalSystem system =
                        store.getResource(
                                     electricalSystemResourceType
                             )
                             .getOrCreate(runtime, world);

                wire = system.resolveWire(wireId);
            }
        }

        if (wire != null) {
            wire.destroy();
        }

        component.setWire(null);
        component.setWireId(null);
    }

    @Override
    public Query<ChunkStore> getQuery() {
        return query;
    }

    private void connectNewWire(
            Wire sourceWire,
            BlockModule.BlockStateInfo blockStateInfo,
            Store<ChunkStore> store,
            CommandBuffer<ChunkStore> commandBuffer
    ) {
        Ref<ChunkStore> sectionRef =
                blockStateInfo.getSectionRef();

        if (!sectionRef.isValid()) {
            return;
        }

        ChunkSection chunkSection =
                commandBuffer.getComponent(
                        sectionRef,
                        ChunkSection.getComponentType()
                );

        BlockSection blockSection =
                commandBuffer.getComponent(
                        sectionRef,
                        BlockSection.getComponentType()
                );

        if (chunkSection == null || blockSection == null) {
            return;
        }

        int blockIndex = blockStateInfo.getIndex();

        int localX = ChunkUtil.xFromIndex(blockIndex);
        int localY = ChunkUtil.yFromIndex(blockIndex);
        int localZ = ChunkUtil.zFromIndex(blockIndex);

        int x = ChunkUtil.worldCoordFromLocalCoord(
                chunkSection.getX(),
                localX
        );

        int y = ChunkUtil.worldCoordFromLocalCoord(
                chunkSection.getY(),
                localY
        );

        int z = ChunkUtil.worldCoordFromLocalCoord(
                chunkSection.getZ(),
                localZ
        );

        int blockTypeIndex =
                blockSection.get(localX, localY, localZ);

        BlockPortDefinition definition =
                portModule.blockPorts(blockTypeIndex);

        if (definition == null) {
            throw new IllegalStateException(
                    "Wire block type "
                            + blockTypeIndex
                            + " has no registered port definition"
            );
        }

        ChunkStore chunkStore = store.getExternalData();
        World world = chunkStore.getWorld();

        HytalePortWorldView worldView =
                new HytalePortWorldView(world);

        LongOpenHashSet connectedWires =
                connectionScratch.get();

        connectedWires.clear();
        connectedWires.add(sourceWire.id().packed());

        PortConnectionConsumer<ElectricalPortConnection> connectionConsumer =
                (
                        targetX,
                        targetY,
                        targetZ,
                        targetPortId,
                        connection,
                        sourceIsResolverFirst
                ) -> {
                    if (connection != ElectricalPortConnection.DIRECT) {
                        return;
                    }

                    Ref<ChunkStore> targetSectionRef =
                            chunkStore
                                    .getChunkSectionReferenceAtBlock(
                                            targetX,
                                            targetY,
                                            targetZ
                                    );

                    if (targetSectionRef == null
                            || !targetSectionRef.isValid()) {
                        return;
                    }

                    Ref<ChunkStore> targetRef =
                            BlockModule.getBlockEntity(
                                    commandBuffer,
                                    targetSectionRef,
                                    targetX,
                                    targetY,
                                    targetZ
                            );

                    if (targetRef == null) {
                        return;
                    }

                    WireComponent targetComponent =
                            commandBuffer.getComponent(
                                    targetRef,
                                    wireComponentType
                            );

                    if (targetComponent == null) {
                        return;
                    }

                    Wire targetWire =
                            targetComponent.getWire();

                    if (targetWire == null) {
                        return;
                    }

                    if (!connectedWires.add(
                            targetWire.id().packed()
                    )) {
                        return;
                    }

                    sourceWire.connect(targetWire);
                };

        try {
            var discovery = portModule.discovery();

            for (int index = 0;
                 index < definition.size();
                 index++) {

                discovery.discover(
                        worldView,
                        x,
                        y,
                        z,
                        definition.portAt(index).localId(),
                        electricalPortDomain,
                        connectionConsumer
                );
            }
        } finally {
            connectedWires.clear();
        }
    }
}