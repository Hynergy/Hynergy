package dev.hynergy.core.electricity;

import com.hypixel.hytale.assetstore.event.LoadedAssetsEvent;
import com.hypixel.hytale.assetstore.event.RemovedAssetsEvent;
import com.hypixel.hytale.assetstore.map.BlockTypeAssetMap;
import com.hypixel.hytale.assetstore.map.DefaultAssetMap;
import com.hypixel.hytale.builtin.asseteditor.event.AssetEditorRequestDataSetEvent;
import com.hypixel.hytale.component.ComponentRegistryProxy;
import com.hypixel.hytale.component.ComponentType;
import com.hypixel.hytale.component.ResourceType;
import com.hypixel.hytale.event.EventRegistry;
import com.hypixel.hytale.logger.HytaleLogger;
import com.hypixel.hytale.server.core.asset.HytaleAssetStore;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType;
import com.hypixel.hytale.server.core.plugin.registry.AssetRegistry;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import dev.hynergy.core.HynergyModule;
import dev.hynergy.core.electricity.wire.WireBlockPortDefinitions;
import dev.hynergy.core.electricity.wire.WireComponent;
import dev.hynergy.core.electricity.wire.WireConfig;
import dev.hynergy.core.electricity.wire.WireSystem;
import dev.hynergy.core.port.PortDomain;
import dev.hynergy.core.port.PortGeometry;
import dev.hynergy.core.port.PortModule;
import dev.hynergy.core.port.PortStandard;
import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceDefinition;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricalRuntime;
import org.joml.Vector3i;
import org.jspecify.annotations.Nullable;

import java.util.Objects;
import java.util.function.Consumer;
import java.util.logging.Level;

public final class ElectricityModule extends HynergyModule {

    private static final HytaleLogger LOGGER = HytaleLogger.forEnclosingClass();

    private final PortModule portModule;
    private final ComponentRegistryProxy<ChunkStore> chunkStoreRegistry;
    private final AssetRegistry assetRegistry;
    private final EventRegistry eventRegistry;

    private @Nullable ElectricalRuntime runtime;
    private @Nullable PortDomain<ElectricalPortConnection> electricalPortDomain;
    private @Nullable PortStandard<ElectricalPortProfile, ElectricalPortConnection> conductorPortStandard;

    private @Nullable WireBlockPortDefinitions wireBlockPortDefinitions;

    private boolean started;

    public ElectricityModule(
            PortModule portModule,
            ComponentRegistryProxy<ChunkStore> chunkStoreRegistry,
            AssetRegistry assetRegistry,
            EventRegistry eventRegistry
    ) {
        this.portModule = Objects.requireNonNull(portModule, "portModule");
        this.chunkStoreRegistry = Objects.requireNonNull(chunkStoreRegistry, "chunkStoreRegistry");
        this.assetRegistry = Objects.requireNonNull(assetRegistry, "assetRegistry");
        this.eventRegistry = Objects.requireNonNull(eventRegistry, "eventRegistry");
    }

    @Override
    public void setup() {
        LOGGER.at(Level.INFO).log("Setting up electricity module");

        registerPortProtocols();

        ComponentType<ChunkStore, WireComponent> wireComponentType =
                chunkStoreRegistry.registerComponent(
                        WireComponent.class,
                        "HynergyWire",
                        WireComponent.CODEC
                );

        wireBlockPortDefinitions =
                new WireBlockPortDefinitions(
                        portModule,
                        conductorPortStandard(),
                        wireComponentType
                );

        assetRegistry.register(
                HytaleAssetStore.builder(
                                        WireConfig.class,
                                        new DefaultAssetMap<>()
                                )
                                .setPath("Hynergy/Electricity/Wires")
                                .setCodec(WireConfig.CODEC)
                                .setKeyFunction(WireConfig::getId)
                                .build()
        );

        eventRegistry.register(
                AssetEditorRequestDataSetEvent.class,
                WireConfig.DATA_SET,
                WireConfig::populateDataSet
        );


        eventRegistry.register(
                LoadedAssetsEvent.class,
                WireConfig.class,
                (Consumer<LoadedAssetsEvent<
                        String,
                        WireConfig,
                        DefaultAssetMap<String, WireConfig>
                        >>) event -> rebuildWireBlockPorts()
        );

        eventRegistry.register(
                RemovedAssetsEvent.class,
                WireConfig.class,
                (Consumer<RemovedAssetsEvent<
                        String,
                        WireConfig,
                        DefaultAssetMap<String, WireConfig>
                        >>) event -> rebuildWireBlockPorts()
        );


        eventRegistry.register(
                LoadedAssetsEvent.class,
                BlockType.class,
                (Consumer<LoadedAssetsEvent<
                        String,
                        BlockType,
                        BlockTypeAssetMap<String, BlockType>
                        >>) event -> rebuildWireBlockPorts()
        );

        eventRegistry.register(
                RemovedAssetsEvent.class,
                BlockType.class,
                (Consumer<RemovedAssetsEvent<
                        String,
                        BlockType,
                        BlockTypeAssetMap<String, BlockType>
                        >>) event -> rebuildWireBlockPorts()
        );

        registerSystems(wireComponentType);
    }

    private void registerPortProtocols() {
        electricalPortDomain = portModule.registerDomain("hynergy:electrical");
        conductorPortStandard = portModule.registerStandard(
                "hynergy:electrical/conductor",
                electricalPortDomain,
                ElectricalPortProfile.class,
                ElectricityModule::resolveConductorConnection
        );
    }

    private void registerSystems(
            ComponentType<ChunkStore, WireComponent> wireComponentType
    ) {
        ResourceType<ChunkStore, ElectricalSystemResource> resourceType =
                chunkStoreRegistry.registerResource(
                        ElectricalSystemResource.class,
                        ElectricalSystemResource::new
                );

        runtime = ElectricalRuntime.create();

        chunkStoreRegistry.registerSystem(
                new ElectricalSystemLifecycleSystem(resourceType)
        );

        chunkStoreRegistry.registerSystem(
                new WireSystem(
                        runtime,
                        resourceType,
                        wireComponentType,
                        portModule,
                        electricalPortDomain()
                )
        );

        chunkStoreRegistry.registerSystem(
                new ElectricalTickSystem(runtime, resourceType)
        );
    }

    private void rebuildWireBlockPorts() {
        WireBlockPortDefinitions definitions =
                wireBlockPortDefinitions;

        if (definitions == null) {
            throw new IllegalStateException(
                    "Wire block port definitions are unavailable before module setup"
            );
        }

        definitions.rebuild();
    }

    @Override
    public void start() {
        rebuildWireBlockPorts();
        started = true;
    }

    @Override
    public void shutdown() {
        if (runtime == null) {
            throw new IllegalStateException("Electrical runtime must be initialized before shutting down.");
        }

        runtime.requestClose();
    }

    public PortDomain<ElectricalPortConnection> electricalPortDomain() {
        PortDomain<ElectricalPortConnection> domain = electricalPortDomain;
        if (domain == null) {
            throw new IllegalStateException("Electrical port domain is unavailable before module setup");
        }
        return domain;
    }

    public PortStandard<ElectricalPortProfile, ElectricalPortConnection> conductorPortStandard() {
        PortStandard<ElectricalPortProfile, ElectricalPortConnection> standard = conductorPortStandard;
        if (standard == null) {
            throw new IllegalStateException("Electrical conductor port standard is unavailable before module setup");
        }
        return standard;
    }

    static @Nullable ElectricalPortConnection resolveConductorConnection(
            ElectricalPortProfile first,
            ElectricalPortProfile second,
            PortGeometry geometry
    ) {
        Vector3i firstNormal = new Vector3i(first.normalX(), first.normalY(), first.normalZ());
        geometry.firstRotation().applyRotationTo(firstNormal);

        if (geometry.ownerDx() != firstNormal.x()
                || geometry.ownerDy() != firstNormal.y()
                || geometry.ownerDz() != firstNormal.z()
                || geometry.anchorDx() != firstNormal.x()
                || geometry.anchorDy() != firstNormal.y()
                || geometry.anchorDz() != firstNormal.z()) {
            return null;
        }

        Vector3i secondNormal = new Vector3i(second.normalX(), second.normalY(), second.normalZ());
        geometry.secondRotation().applyRotationTo(secondNormal);

        if (secondNormal.x() != -firstNormal.x()
                || secondNormal.y() != -firstNormal.y()
                || secondNormal.z() != -firstNormal.z()) {
            return null;
        }

        return ElectricalPortConnection.DIRECT;
    }

    public <T extends Device> DeviceDefinition register(
            DeviceType<T> type
    ) {
        if (started) {
            throw new IllegalStateException("Electrical device types must be registered during setup");
        }

        if (runtime == null) {
            throw new IllegalStateException("Electrical runtime must be initialized before registering a device.");
        }

        return runtime.register(type);
    }
}
