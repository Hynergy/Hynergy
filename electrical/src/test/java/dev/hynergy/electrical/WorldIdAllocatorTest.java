package dev.hynergy.electrical;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

final class WorldIdAllocatorTest {

    private static void commit(WorldIdAllocator ids) {
        ids.prepareCommitBatch();
        ids.commitBatch();
    }

    @Test
    void preparingCommitDoesNotCommitPendingState() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int id = ids.reserve();
        int generation = ids.generation(id);

        ids.prepareCommitBatch();

        assertEquals(0, ids.committedHighWaterMark());

        assertDoesNotThrow(() -> ids.requireUsable(id, generation));

        ids.commitBatch();

        assertEquals(1, ids.committedHighWaterMark());
    }

    @Test
    void freshIdsAreSequentialAndCommitHighWaterMark() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int first = ids.reserve();
        int second = ids.reserve();

        assertEquals(1, first);
        assertEquals(2, second);

        assertEquals(2, ids.highWaterMark());
        assertEquals(0, ids.committedHighWaterMark());

        commit(ids);

        assertEquals(2, ids.highWaterMark());
        assertEquals(2, ids.committedHighWaterMark());

        assertDoesNotThrow(() -> ids.requireUsable(first, ids.generation(first)));
        assertDoesNotThrow(() -> ids.requireUsable(second, ids.generation(second)));
    }

    @Test
    void pendingAddIsUsableBeforeCommit() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int id = ids.reserve();
        int generation = ids.generation(id);

        assertDoesNotThrow(() -> ids.requireUsable(id, generation));
    }

    @Test
    void committedRemovalIsNotReusedInsideSameBatch() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int first = ids.reserve();
        int second = ids.reserve();

        commit(ids);
        ids.remove(first, ids.generation(first));

        assertEquals(3, ids.reserve());

        commit(ids);

        assertEquals(first, ids.reserve());
        assertDoesNotThrow(() -> ids.requireUsable(second, ids.generation(second)));
    }

    @Test
    void sameBatchAddRemoveIsNotReusedUntilCommit() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int first = ids.reserve();

        ids.remove(first, ids.generation(first));

        assertThrows(IllegalStateException.class,
            () -> ids.requireUsable(first, ids.generation(first))
        );

        assertEquals(2, ids.reserve());

        commit(ids);

        assertEquals(2, ids.committedHighWaterMark());
        assertEquals(first, ids.reserve());
    }

    @Test
    void reusingSlotIncrementsGeneration() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int id = ids.reserve();
        int oldGeneration = ids.generation(id);

        commit(ids);

        ids.remove(id, oldGeneration);
        commit(ids);

        assertEquals(id, ids.reserve());
        assertEquals(oldGeneration + 1, ids.generation(id));
    }

    @Test
    void staleGenerationIsRejectedAfterReuse() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int id = ids.reserve();
        int oldGeneration = ids.generation(id);

        commit(ids);

        ids.remove(id, oldGeneration);
        commit(ids);

        assertEquals(id, ids.reserve());

        int newGeneration = ids.generation(id);

        assertNotEquals(oldGeneration, newGeneration);
        assertThrows(IllegalStateException.class, () -> ids.requireUsable(id, oldGeneration));

        assertDoesNotThrow(() -> ids.requireUsable(id, newGeneration));
    }

    @Test
    void removalMakesWrapperImmediatelyUnusable() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int id = ids.reserve();
        int generation = ids.generation(id);

        commit(ids);
        ids.remove(id, generation);

        assertThrows(IllegalStateException.class, () -> ids.requireUsable(id, generation));
    }

    @Test
    void removingSameObjectTwiceFailsBeforeAnotherNativeCommand() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int id = ids.reserve();
        int generation = ids.generation(id);

        commit(ids);

        ids.remove(id, generation);

        assertThrows(IllegalStateException.class, () -> ids.remove(id, generation));
    }

    @Test
    void cancelledFreshReservationRollsBackSpeculativeTail() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int id = ids.reserve();
        int generation = ids.generation(id);

        ids.cancelPendingAdd(id, generation);

        assertEquals(0, ids.highWaterMark());
        assertEquals(0, ids.committedHighWaterMark());

        assertEquals(1, ids.reserve());
        assertEquals(1, ids.generation(1));
    }

    @Test
    void cancelledHoleReservationReturnsHoleToFreePool() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int id = ids.reserve();

        commit(ids);

        ids.remove(id, ids.generation(id));

        commit(ids);

        int reused = ids.reserve();
        int generation = ids.generation(reused);

        assertEquals(id, reused);

        ids.cancelPendingAdd(reused, generation);

        assertEquals(id, ids.reserve());
    }

    @Test
    void cancelledLiveRemovalRestoresUsability() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int id = ids.reserve();
        int generation = ids.generation(id);

        commit(ids);
        ids.remove(id, generation);

        assertThrows(IllegalStateException.class, () -> ids.requireUsable(id, generation));

        ids.cancelPendingRemove(id, generation);

        assertDoesNotThrow(() -> ids.requireUsable(id, generation));

        commit(ids);

        assertDoesNotThrow(() -> ids.requireUsable(id, generation));
    }

    @Test
    void cancelledPendingAddRemovalRestoresPendingAdd() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int id = ids.reserve();
        int generation = ids.generation(id);

        ids.remove(id, generation);

        assertThrows(IllegalStateException.class, () -> ids.requireUsable(id, generation));

        ids.cancelPendingRemove(id, generation);

        assertDoesNotThrow(() -> ids.requireUsable(id, generation));

        commit(ids);

        assertDoesNotThrow(() -> ids.requireUsable(id, generation));
    }

    @Test
    void cancelledFreshAddMustBeMostRecentAllocatorTransition() {
        WorldIdAllocator ids = new WorldIdAllocator();

        int first = ids.reserve();
        int firstGeneration = ids.generation(first);

        ids.reserve();

        assertThrows(IllegalStateException.class,
            () -> ids.cancelPendingAdd(first, firstGeneration)
        );
    }

    @Test
    void unknownIdIsRejected() {
        WorldIdAllocator ids = new WorldIdAllocator();

        assertThrows(IllegalStateException.class, () -> ids.generation(1));

        int id = ids.reserve();

        assertThrows(IllegalStateException.class, () -> ids.requireUsable(id + 1, 1));
    }
}