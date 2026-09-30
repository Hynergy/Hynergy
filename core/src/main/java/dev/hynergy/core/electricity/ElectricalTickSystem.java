package dev.hynergy.core.electricity;

import com.hypixel.hytale.component.ResourceType;
import com.hypixel.hytale.component.Store;
import com.hypixel.hytale.component.system.tick.TickingSystem;
import com.hypixel.hytale.server.core.universe.world.World;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import dev.hynergy.electrical.ElectricalRuntime;
import dev.hynergy.electrical.ElectricalSystem;

final class ElectricalTickSystem extends TickingSystem<ChunkStore> {
    private final ElectricalRuntime runtime;
    private final ResourceType<ChunkStore, ElectricalSystemResource> resourceType;

    public ElectricalTickSystem(
        ElectricalRuntime runtime,
        ResourceType<ChunkStore, ElectricalSystemResource> resourceType
    ) {
        this.runtime = runtime;
        this.resourceType = resourceType;
    }

    @Override
    public void tick(float v, int i, Store<ChunkStore> store) {
        store.assertThread();

        World world = store.getExternalData().getWorld();

        world.debugAssertInTickingThread();

        ElectricalSystemResource resource = store.getResource(resourceType);
        ElectricalSystem system = resource.getOrCreate(runtime, world);

        system.tick();
    }
}
