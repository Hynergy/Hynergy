package dev.hynergy.core.electricity;

import com.hypixel.hytale.component.Resource;
import com.hypixel.hytale.server.core.universe.world.World;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import dev.hynergy.electrical.ElectricalRuntime;
import dev.hynergy.electrical.ElectricalSystem;
import org.checkerframework.checker.nullness.compatqual.NullableDecl;
import org.jspecify.annotations.Nullable;

public class ElectricalSystemResource implements Resource<ChunkStore> {
    private @Nullable ElectricalSystem system;

    public ElectricalSystem getOrCreate(ElectricalRuntime runtime, World world) {
        ElectricalSystem system = this.system;

        if (system == null) {
            system = runtime.createSystem(world.getTps());
            this.system = system;
        }

        return system;
    }

    void close() {
        ElectricalSystem system = this.system;

        if (system == null) {
            return;
        }

        system.close();
        this.system = null;
    }

    @NullableDecl
    @Override
    public Resource<ChunkStore> clone() {
        try {
            return (ElectricalSystemResource) super.clone();
        } catch (CloneNotSupportedException e) {
            throw new AssertionError(e);
        }
    }
}
