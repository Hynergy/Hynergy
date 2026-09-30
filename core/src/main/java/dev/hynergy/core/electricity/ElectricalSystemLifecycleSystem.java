package dev.hynergy.core.electricity;

import com.hypixel.hytale.component.ResourceType;
import com.hypixel.hytale.component.Store;
import com.hypixel.hytale.component.system.StoreSystem;
import com.hypixel.hytale.server.core.universe.world.World;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;

public class ElectricalSystemLifecycleSystem extends StoreSystem<ChunkStore> {
    private final ResourceType<ChunkStore, ElectricalSystemResource> resourceType;

    public ElectricalSystemLifecycleSystem(
        ResourceType<ChunkStore, ElectricalSystemResource> resourceType
    ) {
        this.resourceType = resourceType;
    }

    @Override
    public void onSystemAddedToStore(
        Store<ChunkStore> store
    ) {
    }

    @Override
    public void onSystemRemovedFromStore(
        Store<ChunkStore> store
    ) {
        store.assertThread();

        World world = store.getExternalData().getWorld();

        world.debugAssertInTickingThread();

        store.getResource(resourceType).close();
    }
}
