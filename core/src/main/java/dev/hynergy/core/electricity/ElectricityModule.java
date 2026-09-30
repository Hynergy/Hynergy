package dev.hynergy.core.electricity;

import com.hypixel.hytale.component.ComponentRegistryProxy;
import com.hypixel.hytale.component.ResourceType;
import com.hypixel.hytale.logger.HytaleLogger;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import dev.hynergy.core.HynergyModule;
import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceDefinition;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricalRuntime;
import org.jspecify.annotations.Nullable;

import java.util.logging.Level;

public final class ElectricityModule extends HynergyModule {

    private static final HytaleLogger LOGGER = HytaleLogger.forEnclosingClass();

    private final ComponentRegistryProxy<ChunkStore> chunkStoreRegistry;
    private @Nullable ElectricalRuntime runtime;

    private boolean started;


    public ElectricityModule(
        ComponentRegistryProxy<ChunkStore> chunkStoreRegistry
    ) {
        this.chunkStoreRegistry = chunkStoreRegistry;
    }

    @Override
    public void setup() {
        LOGGER.at(Level.INFO).log("Setting up electricity module");

        runtime = ElectricalRuntime.create();

        ResourceType<ChunkStore, ElectricalSystemResource> resourceType =
            chunkStoreRegistry.registerResource(ElectricalSystemResource.class, ElectricalSystemResource::new);

        chunkStoreRegistry.registerSystem(new ElectricalSystemLifecycleSystem(resourceType));
        chunkStoreRegistry.registerSystem(new ElectricalTickSystem(runtime, resourceType));
    }

    @Override
    public void start() {
        started = true;
    }

    @Override
    public void shutdown() {
        if (runtime == null) {
            throw new IllegalStateException("Electrical runtime must be initialized before shutting down.");
        }

        runtime.requestClose();
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
