package dev.hynergy.electrical.internal;

import java.lang.foreign.*;
import java.lang.invoke.MethodHandle;

final class NativeBindings {
    private static final Linker LINKER = Linker.nativeLinker();
    private static final SymbolLookup SYMBOLS = NativeLibrary.load();

    private static final MethodHandle ABI_VERSION = downcall("hynergy_abi_version", FunctionDescriptor.of(ValueLayout.JAVA_INT));
    private static final MethodHandle ABI_REVISION = downcall("hynergy_abi_revision ", FunctionDescriptor.of(ValueLayout.JAVA_INT));


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
}
