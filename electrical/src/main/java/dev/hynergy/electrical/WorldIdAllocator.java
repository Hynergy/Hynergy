package dev.hynergy.electrical;

import java.util.Arrays;

final class WorldIdAllocator {
    static final int MAX_ID = Integer.MAX_VALUE;

    private static final byte UNUSED = 0;
    private static final byte FREE = 1;
    private static final byte LIVE = 2;
    private static final byte PENDING_ADD = 3;
    private static final byte PENDING_REMOVE = 4;
    private static final byte PENDING_ADD_REMOVE = 5;
    private static final byte RETIRED = 6;

    private static final int INITIAL_SLOT_CAPACITY = 16;
    private static final int INITIAL_LIST_CAPACITY = 8;

    private byte[] states = new byte[INITIAL_SLOT_CAPACITY];
    private int[] generations = new int[INITIAL_SLOT_CAPACITY];

    private int[] freeIds = new int[INITIAL_LIST_CAPACITY];
    private int freeCount;

    private int[] pendingIds = new int[INITIAL_LIST_CAPACITY];
    private int pendingCount;

    private int highWaterMark;
    private int committedHighWaterMark;

    int reserve() {
        while (freeCount != 0) {
            int id = freeIds[freeCount - 1];
            int index = id - 1;

            if (states[index] != FREE) {
                throw new IllegalStateException("Free ID pool is inconsistent");
            }

            int nextGeneration = generations[index] + 1;

            if (nextGeneration == 0) {
                freeCount--;
                states[index] = RETIRED;
                continue;
            }

            ensurePendingCapacity(1);

            freeCount--;
            generations[index] = nextGeneration;
            states[index] = PENDING_ADD;
            pendingIds[pendingCount++] = id;

            return id;
        }

        if (highWaterMark == MAX_ID) {
            throw new IllegalStateException("World ID space is exhausted");
        }

        int id = highWaterMark + 1;
        int index = id - 1;

        ensureSlotCapacity(id);
        ensurePendingCapacity(1);

        if (states[index] != UNUSED || generations[index] != 0) {
            throw new IllegalStateException("Fresh ID slot is inconsistent");
        }

        generations[index] = 1;
        states[index] = PENDING_ADD;
        highWaterMark = id;
        pendingIds[pendingCount++] = id;

        return id;
    }

    int generation(int id) {
        return generations[requireKnownId(id)];
    }

    void requireUsable(int id, int generation) {
        int index = requireCurrentGeneration(id, generation);
        byte state = states[index];

        if (state != LIVE && state != PENDING_ADD) {
            throw new IllegalStateException("World object is not usable");
        }
    }

    void remove(int id, int generation) {
        int index = requireCurrentGeneration(id, generation);

        switch (states[index]) {
            case LIVE -> {
                ensurePendingCapacity(1);

                states[index] = PENDING_REMOVE;
                pendingIds[pendingCount++] = id;
            }

            case PENDING_ADD -> states[index] = PENDING_ADD_REMOVE;

            case PENDING_REMOVE, PENDING_ADD_REMOVE, FREE, RETIRED, UNUSED ->
                    throw new IllegalStateException("World object is not removable");

            default -> throw new IllegalStateException("Unknown world ID state");
        }
    }

    void cancelPendingAdd(int id, int generation) {
        int index = requireCurrentGeneration(id, generation);

        if (states[index] != PENDING_ADD) {
            throw new IllegalStateException("World object does not have a pending add");
        }

        if (pendingCount == 0 || pendingIds[pendingCount - 1] != id) {
            throw new IllegalStateException("Only the most recent pending add can be cancelled");
        }

        if (id > committedHighWaterMark) {
            if (id != highWaterMark) {
                throw new IllegalStateException(
                        "Speculative tail allocation is not the high-water ID");
            }

            pendingCount--;

            states[index] = UNUSED;
            generations[index] = 0;

            highWaterMark--;

            return;
        }

        ensureFreeCapacity(1);

        pendingCount--;

        states[index] = FREE;
        freeIds[freeCount++] = id;
    }

    void cancelPendingRemove(int id, int generation) {
        int index = requireCurrentGeneration(id, generation);

        switch (states[index]) {
            case PENDING_REMOVE -> {
                if (pendingCount == 0 || pendingIds[pendingCount - 1] != id) {
                    throw new IllegalStateException(
                            "Only the most recent pending removal can be cancelled");
                }

                pendingCount--;
                states[index] = LIVE;
            }

            case PENDING_ADD_REMOVE -> states[index] = PENDING_ADD;

            default -> throw new IllegalStateException("World object does not have a pending removal");
        }
    }

    void prepareCommitBatch() {
        int reusableReleaseCount = validatePendingAndCountReusableReleases();

        ensureFreeCapacity(reusableReleaseCount);
    }

    void commitBatch() {
        for (int i = 0; i < pendingCount; i++) {
            int id = pendingIds[i];
            int index = id - 1;

            switch (states[index]) {
                case PENDING_ADD -> states[index] = LIVE;

                case PENDING_REMOVE, PENDING_ADD_REMOVE -> releaseCommitted(id, index);

                default -> throw new AssertionError("Pending ID state changed after commit preparation");
            }
        }

        pendingCount = 0;
        committedHighWaterMark = highWaterMark;
    }

    private int validatePendingAndCountReusableReleases() {
        int reusableReleaseCount = 0;

        for (int i = 0; i < pendingCount; i++) {
            int index = pendingIds[i] - 1;

            switch (states[index]) {
                case PENDING_ADD -> {
                }

                case PENDING_REMOVE, PENDING_ADD_REMOVE -> {
                    if (generations[index] != -1) {
                        reusableReleaseCount++;
                    }
                }

                default -> throw new IllegalStateException("Pending ID state is inconsistent");
            }
        }

        return reusableReleaseCount;
    }

    int highWaterMark() {
        return highWaterMark;
    }

    int committedHighWaterMark() {
        return committedHighWaterMark;
    }

    private void releaseCommitted(int id, int index) {
        if (generations[index] == -1) {
            states[index] = RETIRED;
            return;
        }

        states[index] = FREE;
        freeIds[freeCount++] = id;
    }

    private int requireKnownId(int id) {
        if (id <= 0 || id > highWaterMark) {
            throw new IllegalStateException("World ID is not allocated");
        }

        int index = id - 1;

        if (states[index] == UNUSED) {
            throw new IllegalStateException("World ID is not allocated");
        }

        return index;
    }

    private int requireCurrentGeneration(int id, int generation) {
        int index = requireKnownId(id);

        if (generations[index] != generation) {
            throw new IllegalStateException("World object generation is stale");
        }

        return index;
    }

    private void ensureSlotCapacity(int requiredSlots) {
        if (requiredSlots <= states.length) {
            return;
        }

        int newCapacity = growCapacity(states.length, requiredSlots);

        byte[] newStates = Arrays.copyOf(states, newCapacity);

        int[] newGenerations = Arrays.copyOf(generations, newCapacity);

        states = newStates;
        generations = newGenerations;
    }

    private void ensurePendingCapacity(int additional) {
        int required = requiredCapacity(pendingCount, additional);

        if (required <= pendingIds.length) {
            return;
        }

        pendingIds = Arrays.copyOf(pendingIds, growCapacity(pendingIds.length, required));
    }

    private void ensureFreeCapacity(int additional) {
        int required = requiredCapacity(freeCount, additional);

        if (required <= freeIds.length) {
            return;
        }

        freeIds = Arrays.copyOf(freeIds, growCapacity(freeIds.length, required));
    }

    private static int requiredCapacity(int current, int additional) {
        if (additional < 0) {
            throw new IllegalArgumentException("Additional capacity must not be negative");
        }

        long required = (long) current + additional;

        if (required > MAX_ID) {
            throw new IllegalStateException("World ID allocator capacity is exhausted");
        }

        return (int) required;
    }

    private static int growCapacity(int current, int required) {
        long grown = (long) current + (current >>> 1);
        long capacity = Math.max(grown, required);

        return (int) Math.min(capacity, MAX_ID);
    }
}