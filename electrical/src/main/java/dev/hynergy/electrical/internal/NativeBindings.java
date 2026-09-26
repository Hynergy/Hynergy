package dev.hynergy.electrical.internal;

import java.lang.foreign.*;
import java.lang.invoke.MethodHandle;

public final class NativeBindings {
    private static final Linker LINKER = Linker.nativeLinker();
    private static final SymbolLookup SYMBOLS = NativeLibrary.load();

    private static final MethodHandle ABI_VERSION = downcall("hynergy_abi_version", FunctionDescriptor.of(ValueLayout.JAVA_INT));
    private static final MethodHandle ABI_REVISION = downcall("hynergy_abi_revision ", FunctionDescriptor.of(ValueLayout.JAVA_INT));

    private static final MethodHandle ENGINE_CREATE = downcall(
            "hynergy_engine_create",
            FunctionDescriptor.of(
                    ValueLayout.ADDRESS, // return EngineHandle*
                    ValueLayout.JAVA_INT // max_worker_threads
            )
    );
    private static final MethodHandle ENGINE_DESTROY = downcall("hynergy_engine_destroy", FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    private static final MethodHandle ENGINE_CREATE_WORLD = downcall(
            "hynergy_engine_create_world",
            FunctionDescriptor.of(
                    ValueLayout.JAVA_INT, // return u32
                    ValueLayout.ADDRESS,  // EngineHandle*
                    ValueLayout.JAVA_INT, // tick_frequency_hz
                    ValueLayout.ADDRESS   // WorldCreationResult*
            )
    );
    private static final MethodHandle WORLD_DESTROY = downcall("hynergy_world_destroy", FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    private static MethodHandle downcall(
            String name,
            FunctionDescriptor descriptor
    ) {
        MemorySegment symbol = SYMBOLS.find(name)
                .orElseThrow(() -> new IllegalStateException(
                        "Missing native symbol: " + name
                ));

        return LINKER.downcallHandle(symbol, descriptor);
    }

    static int abiVersion() {
        try {
            return (int) ABI_VERSION.invokeExact();
        } catch (Throwable e) {
            throw new IllegalStateException(
                    "Failed to call hynergy_abi_version",
                    e
            );
        }
    }

    static int abiRevision() {
        try {
            return (int) ABI_REVISION.invokeExact();
        } catch (Throwable e) {
            throw new IllegalStateException(
                    "Failed to call hynergy_abi_revision",
                    e
            );
        }
    }

    public static MemorySegment createEngine(int maxWorkerThreads) {
        try {
            return (MemorySegment) ENGINE_CREATE.invokeExact(maxWorkerThreads);
        } catch (Throwable throwable) {
            throw new IllegalStateException(
                    "Failed to create native electrical engine",
                    throwable
            );
        }
    }

    public static void destroyEngine(MemorySegment engine) {
        try {
            ENGINE_DESTROY.invokeExact(engine);
        } catch (Throwable throwable) {
            throw new IllegalStateException(
                    "Failed to destroy native electrical engine",
                    throwable
            );
        }
    }

    public static int createWorld(
            MemorySegment engine,
            int tickFrequencyHz,
            MemorySegment result
    ) {
        try {
            return (int) ENGINE_CREATE_WORLD.invokeExact(
                    engine,
                    tickFrequencyHz,
                    result
            );
        } catch (Throwable throwable) {
            throw new IllegalStateException(
                    "Failed to create native electrical world",
                    throwable
            );
        }
    }

    public static void destroyWorld(MemorySegment world) {
        try {
            WORLD_DESTROY.invokeExact(world);
        } catch (Throwable throwable) {
            throw new IllegalStateException(
                    "Failed to destroy native electrical world",
                    throwable
            );
        }
    }
}
