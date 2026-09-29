package dev.hynergy.electrical;

import org.jspecify.annotations.Nullable;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.ByteOrder;
import java.util.Objects;

/**
 * Encodes a device definition in the HYDF format.
 *
 * <p>The builder writes directly to native memory. The builder keeps its
 * current buffer capacity when {@link #reset()} resets the definition.</p>
 *
 * <p>The builder is reusable. The builder is not thread-safe. Use the builder
 * on the thread that creates it.</p>
 *
 * <p>Close the builder when it is no longer necessary.</p>
 */
public final class DeviceDefinitionBuilder implements AutoCloseable {

    /**
     * Specifies one parameter bound.
     *
     * @param value the bound value
     * @param inclusive {@code true} when the bound includes the value
     */
    public record Bound(double value, boolean inclusive) {

        /**
         * Creates an inclusive bound.
         *
         * @param value the bound value
         *
         * @return the bound
         */
        public static Bound inclusive(double value) {
            return new Bound(value, true);
        }

        /**
         * Creates an exclusive bound.
         *
         * @param value the bound value
         *
         * @return the bound
         */
        public static Bound exclusive(double value) {
            return new Bound(value, false);
        }
    }

    private static final long UINT32_MAX = 0xffff_ffffL;

    private static final int DEFAULT_INITIAL_CAPACITY = 256;

    private static final int HEADER_SIZE = 16;
    private static final int COMMAND_HEADER_SIZE = 6;

    private static final long COMMAND_COUNT_OFFSET = 12;

    private static final int COMMAND_ADD_TERMINAL = 1;
    private static final int COMMAND_ADD_NODE = 2;
    private static final int COMMAND_ADD_PARAMETER = 3;
    private static final int COMMAND_ADD_ELEMENT = 4;
    private static final int COMMAND_ADD_VOLTAGE_OBSERVER = 5;
    private static final int COMMAND_ADD_CHILD_OBSERVER = 6;

    private static final int VALUE_LITERAL = 0;
    private static final int VALUE_PARAMETER = 1;

    private static final int CONSTRAINT_LOWER = 1;
    private static final int CONSTRAINT_LOWER_INCLUSIVE = 1 << 1;

    private static final int CONSTRAINT_UPPER = 1 << 2;
    private static final int CONSTRAINT_UPPER_INCLUSIVE = 1 << 3;

    private static final int CONSTRAINT_NON_ZERO = 1 << 4;

    private static final int CONSTRAINT_RECIPROCAL_RANGE = 1 << 5;

    private static final int CONSTRAINT_RECIPROCAL_LOWER = 1 << 6;
    private static final int CONSTRAINT_RECIPROCAL_LOWER_INCLUSIVE = 1 << 7;

    private static final int CONSTRAINT_RECIPROCAL_UPPER = 1 << 8;
    private static final int CONSTRAINT_RECIPROCAL_UPPER_INCLUSIVE = 1 << 9;

    private static final ValueLayout.OfShort U16 = ValueLayout.JAVA_SHORT_UNALIGNED.withOrder(ByteOrder.LITTLE_ENDIAN);

    private static final ValueLayout.OfInt U32 = ValueLayout.JAVA_INT_UNALIGNED.withOrder(ByteOrder.LITTLE_ENDIAN);

    private static final ValueLayout.OfDouble F64 =
        ValueLayout.JAVA_DOUBLE_UNALIGNED.withOrder(ByteOrder.LITTLE_ENDIAN);

    private @Nullable Arena arena;
    private MemorySegment buffer;

    private long position;
    private long commandCount;

    private long nodeCount;
    private long parameterCount;
    private long elementCount;
    private long observerCount;

    private boolean elementOpen;
    private boolean elementParametersStarted;

    private long elementCommandOffset;
    private long elementTerminalCountOffset;
    private long elementParameterCountOffset;

    private long elementTerminalCount;
    private long elementParameterCount;

    /**
     * Creates a builder with the default initial buffer capacity.
     */
    public DeviceDefinitionBuilder() {
        this(DEFAULT_INITIAL_CAPACITY);
    }

    /**
     * Creates a builder with the specified initial buffer capacity.
     *
     * <p>This constructor has package access to support tests that must force
     * buffer growth.</p>
     *
     * @param initialCapacity the initial capacity in bytes
     *
     * @throws IllegalArgumentException if the capacity is smaller than the
     *     HYDF header
     */
    DeviceDefinitionBuilder(int initialCapacity) {
        if (initialCapacity < HEADER_SIZE) {
            throw new IllegalArgumentException("Initial capacity must be at least " + HEADER_SIZE + " bytes");
        }

        Arena createdArena = Arena.ofConfined();

        try {
            buffer = createdArena.allocate(initialCapacity, 1);
        } catch (RuntimeException | Error failure) {
            try {
                createdArena.close();
            } catch (RuntimeException | Error closeFailure) {
                failure.addSuppressed(closeFailure);
            }

            throw failure;
        }

        arena = createdArena;

        resetState();
    }

    /**
     * Resets the definition.
     *
     * <p>The method keeps the current buffer capacity.</p>
     *
     * @return this builder
     *
     * @throws IllegalStateException if the builder is closed
     */
    public DeviceDefinitionBuilder reset() {
        requireOpen();
        resetState();

        return this;
    }

    /**
     * Resets the encoded definition state and writes an empty HYDF header.
     */
    private void resetState() {
        position = HEADER_SIZE;
        commandCount = 0;

        nodeCount = 0;
        parameterCount = 0;
        elementCount = 0;
        observerCount = 0;

        clearElementState();
        writeHeader();
    }

    /**
     * Adds one terminal.
     *
     * <p>The operation also creates the node for the terminal.</p>
     *
     * @return the node ID
     *
     * @throws IllegalStateException if an element is open or the builder is
     *     closed
     */
    public int addTerminal() {
        requireTopLevel();

        int nodeId = nextNodeId();

        long payload = prepareCommand(COMMAND_ADD_TERMINAL, 0);

        commitCommand(payload);

        nodeCount++;

        return nodeId;
    }

    /**
     * Adds one internal node.
     *
     * @return the node ID
     *
     * @throws IllegalStateException if an element is open or the builder is
     *     closed
     */
    public int addNode() {
        requireTopLevel();

        int nodeId = nextNodeId();

        long payload = prepareCommand(COMMAND_ADD_NODE, 0);

        commitCommand(payload);

        nodeCount++;

        return nodeId;
    }

    /**
     * Adds an unrestricted parameter.
     *
     * @return the parameter ID
     */
    public int addParameter() {
        return addParameter(null, null, false);
    }

    /**
     * Adds a parameter.
     *
     * @param lower the lower bound, or {@code null}
     * @param upper the upper bound, or {@code null}
     * @param nonZero {@code true} if zero is not valid
     *
     * @return the parameter ID
     */
    public int addParameter(@Nullable Bound lower, @Nullable Bound upper, boolean nonZero) {
        return addParameter(lower, upper, nonZero, false, null, null);
    }

    /**
     * Adds a parameter with reciprocal constraints.
     *
     * <p>The reciprocal range is present even when both reciprocal bounds are
     * {@code null}. An empty reciprocal range requires a finite reciprocal.</p>
     *
     * @param lower the lower bound, or {@code null}
     * @param upper the upper bound, or {@code null}
     * @param nonZero {@code true} if zero is not valid
     * @param reciprocalLower the reciprocal lower bound, or {@code null}
     * @param reciprocalUpper the reciprocal upper bound, or {@code null}
     *
     * @return the parameter ID
     */
    public int addParameterWithReciprocalRange(
        @Nullable Bound lower,
        @Nullable Bound upper,
        boolean nonZero,
        @Nullable Bound reciprocalLower,
        @Nullable Bound reciprocalUpper
    ) {
        return addParameter(lower, upper, nonZero, true, reciprocalLower, reciprocalUpper);
    }

    /**
     * Starts an element.
     *
     * <p>Add all element terminals before the first element parameter. Call
     * {@link #endElement()} to finish the element.</p>
     *
     * @param definition the child device definition
     *
     * @return this builder
     */
    public DeviceDefinitionBuilder beginElement(
        DeviceDefinition definition
    ) {
        requireTopLevel();
        Objects.requireNonNull(definition, "definition");

        int definitionId = definition.id();

        if (definitionId == 0) {
            throw new IllegalArgumentException("Definition ID must not be zero");
        }

        requireCommandAvailable();

        ensureWritable(COMMAND_HEADER_SIZE + Integer.BYTES + Integer.BYTES);

        long commandOffset = position;

        putU16(commandOffset, COMMAND_ADD_ELEMENT);

        putU32(commandOffset + Short.BYTES, 0);

        long payloadOffset = commandOffset + COMMAND_HEADER_SIZE;

        putU32Raw(payloadOffset, definitionId);

        elementTerminalCountOffset = payloadOffset + Integer.BYTES;

        putU32(elementTerminalCountOffset, 0);

        position = elementTerminalCountOffset + Integer.BYTES;

        elementOpen = true;
        elementParametersStarted = false;

        elementCommandOffset = commandOffset;
        elementParameterCountOffset = -1;

        elementTerminalCount = 0;
        elementParameterCount = 0;

        return this;
    }

    /**
     * Adds one terminal mapping to the current element.
     *
     * @param nodeId the node ID
     *
     * @return this builder
     */
    public DeviceDefinitionBuilder elementTerminal(
        int nodeId
    ) {
        requireElementOpen();

        if (elementParametersStarted) {
            throw new IllegalStateException("Element terminals must be added before parameters");
        }

        requireExistingId(nodeId, nodeCount, "Node");

        ensureWritable(Integer.BYTES);

        putU32Raw(position, nodeId);

        position += Integer.BYTES;
        elementTerminalCount++;

        return this;
    }

    /**
     * Adds one literal parameter value to the current element.
     *
     * @param value the parameter value
     *
     * @return this builder
     *
     * @throws IllegalArgumentException if the value is not finite
     */
    public DeviceDefinitionBuilder elementLiteral(
        double value
    ) {
        requireElementOpen();

        if (!Double.isFinite(value)) {
            throw new IllegalArgumentException("Element literal must be finite");
        }

        beginElementParameters();

        ensureWritable(Byte.BYTES + Double.BYTES);

        buffer.set(ValueLayout.JAVA_BYTE, position, (byte) VALUE_LITERAL);

        putF64(position + Byte.BYTES, value);

        position += Byte.BYTES + Double.BYTES;

        elementParameterCount++;

        return this;
    }

    /**
     * Adds one definition parameter reference to the current element.
     *
     * @param parameterId the parameter ID
     *
     * @return this builder
     */
    public DeviceDefinitionBuilder elementParameter(
        int parameterId
    ) {
        requireElementOpen();

        requireExistingId(parameterId, parameterCount, "Parameter");

        beginElementParameters();

        ensureWritable(Byte.BYTES + Integer.BYTES);

        buffer.set(ValueLayout.JAVA_BYTE, position, (byte) VALUE_PARAMETER);

        putU32Raw(position + Byte.BYTES, parameterId);

        position += Byte.BYTES + Integer.BYTES;

        elementParameterCount++;

        return this;
    }

    /**
     * Finishes the current element.
     *
     * @return the element ID
     */
    public int endElement() {
        requireElementOpen();

        beginElementParameters();

        putU32(elementParameterCountOffset, elementParameterCount);

        long payloadOffset = elementCommandOffset + COMMAND_HEADER_SIZE;
        long payloadLength = position - payloadOffset;

        putU32(elementCommandOffset + Short.BYTES, payloadLength);

        long nextCommandCount = commandCount + 1;

        putU32(COMMAND_COUNT_OFFSET, nextCommandCount);

        int elementId = (int) elementCount;

        commandCount = nextCommandCount;
        elementCount++;

        clearElementState();

        return elementId;
    }

    /**
     * Adds one voltage observer.
     *
     * @param positiveNode the positive node ID
     * @param negativeNode the negative node ID
     *
     * @return the observer ID
     */
    public int addVoltageObserver(int positiveNode, int negativeNode) {
        requireTopLevel();

        requireExistingId(positiveNode, nodeCount, "Positive node");
        requireExistingId(negativeNode, nodeCount, "Negative node");

        int observerId = nextObserverId();

        long payload = prepareCommand(COMMAND_ADD_VOLTAGE_OBSERVER, 2L * Integer.BYTES);

        putU32Raw(payload, positiveNode);
        putU32Raw(payload + Integer.BYTES, negativeNode);

        commitCommand(payload + 2L * Integer.BYTES);

        observerCount++;

        return observerId;
    }

    /**
     * Adds one observer that uses an observer from a child element.
     *
     * @param elementId the element ID
     * @param observerId the observer ID in the child definition
     *
     * @return the observer ID in this definition
     */
    public int addChildObserver(int elementId, int observerId) {
        requireTopLevel();

        requireExistingId(elementId, elementCount, "Element");

        int newObserverId = nextObserverId();

        long payload = prepareCommand(COMMAND_ADD_CHILD_OBSERVER, 2L * Integer.BYTES);

        putU32Raw(payload, elementId);
        putU32Raw(payload + Integer.BYTES, observerId);

        commitCommand(payload + 2L * Integer.BYTES);

        observerCount++;

        return newObserverId;
    }

    /**
     * Returns the encoded HYDF definition.
     *
     * <p>The returned segment uses this builder's memory. The segment becomes
     * invalid when the builder grows its buffer or closes.</p>
     *
     * @return the encoded definition
     */
    MemorySegment encodedSegment() {
        requireComplete();

        return buffer.asSlice(0, position);
    }

    /**
     * Returns the encoded size as an unsigned 32-bit value stored in a Java
     * {@code int}.
     *
     * @return the encoded size
     */
    int encodedLength() {
        requireComplete();

        return (int) position;
    }

    /**
     * Returns the encoded size in bytes.
     *
     * @return the encoded size
     */
    long byteSize() {
        requireComplete();

        return position;
    }

    /**
     * Returns the current buffer capacity.
     *
     * @return the capacity in bytes
     */
    long capacity() {
        requireOpen();

        return buffer.byteSize();
    }

    /**
     * Returns the current HYDF command count.
     *
     * @return the command count
     */
    long commandCount() {
        requireComplete();

        return commandCount;
    }

    private int addParameter(
        @Nullable Bound lower,
        @Nullable Bound upper,
        boolean nonZero,
        boolean reciprocalRange,
        @Nullable Bound reciprocalLower,
        @Nullable Bound reciprocalUpper
    ) {
        requireTopLevel();

        int parameterId = nextParameterId();

        int flags = boundFlags(lower, CONSTRAINT_LOWER, CONSTRAINT_LOWER_INCLUSIVE) | boundFlags(
            upper,
            CONSTRAINT_UPPER,
            CONSTRAINT_UPPER_INCLUSIVE
        );

        if (nonZero) {
            flags |= CONSTRAINT_NON_ZERO;
        }

        if (reciprocalRange) {
            flags |= CONSTRAINT_RECIPROCAL_RANGE;
            flags |= boundFlags(reciprocalLower, CONSTRAINT_RECIPROCAL_LOWER, CONSTRAINT_RECIPROCAL_LOWER_INCLUSIVE);
            flags |= boundFlags(reciprocalUpper, CONSTRAINT_RECIPROCAL_UPPER, CONSTRAINT_RECIPROCAL_UPPER_INCLUSIVE);
        }

        int boundCount =
            countBound(lower) + countBound(upper) + (reciprocalRange ? countBound(reciprocalLower) + countBound(
                reciprocalUpper) : 0);

        long payloadLength = Short.BYTES + (long) boundCount * Double.BYTES;

        long payload = prepareCommand(COMMAND_ADD_PARAMETER, payloadLength);

        putU16(payload, flags);

        long cursor = payload + Short.BYTES;

        cursor = writeBound(cursor, lower);
        cursor = writeBound(cursor, upper);

        if (reciprocalRange) {
            cursor = writeBound(cursor, reciprocalLower);
            cursor = writeBound(cursor, reciprocalUpper);
        }

        commitCommand(cursor);

        parameterCount++;

        return parameterId;
    }

    /**
     * Returns the HYDF flags for one optional bound.
     *
     * @param bound the bound, or {@code null}
     * @param presentFlag the flag that marks the bound as present
     * @param inclusiveFlag the flag that marks the bound as inclusive
     *
     * @return the encoded flags
     */
    private static int boundFlags(@Nullable Bound bound, int presentFlag, int inclusiveFlag) {
        if (bound == null) {
            return 0;
        }

        return presentFlag | (bound.inclusive() ? inclusiveFlag : 0);
    }

    /**
     * Returns the number of values for one optional bound.
     *
     * @param bound the bound, or {@code null}
     *
     * @return {@code 1} when the bound is present, otherwise {@code 0}
     */
    private static int countBound(
        @Nullable Bound bound
    ) {
        return bound == null ? 0 : 1;
    }

    /**
     * Writes one optional bound.
     *
     * @param cursor the current write offset
     * @param bound the bound, or {@code null}
     *
     * @return the next write offset
     */
    private long writeBound(long cursor, @Nullable Bound bound) {
        if (bound == null) {
            return cursor;
        }

        putF64(cursor, bound.value());

        return cursor + Double.BYTES;
    }

    /**
     * Starts the parameter section of the current element.
     */
    private void beginElementParameters() {
        if (elementParametersStarted) {
            return;
        }

        ensureWritable(Integer.BYTES);

        putU32(elementTerminalCountOffset, elementTerminalCount);

        elementParameterCountOffset = position;

        putU32(elementParameterCountOffset, 0);

        position += Integer.BYTES;
        elementParametersStarted = true;
    }

    /**
     * Writes one command header.
     *
     * @param tag the command tag
     * @param payloadLength the payload size
     *
     * @return the payload offset
     */
    private long prepareCommand(int tag, long payloadLength) {
        requireTopLevel();
        requireCommandAvailable();

        if (payloadLength < 0 || payloadLength > UINT32_MAX) {

            throw new IllegalArgumentException("Command payload exceeds the HYDF u32 length range");
        }

        long totalLength = COMMAND_HEADER_SIZE + payloadLength;

        long end = checkedEnd(position, totalLength);

        ensureCapacity(end);

        putU16(position, tag);
        putU32(position + Short.BYTES, payloadLength);

        return position + COMMAND_HEADER_SIZE;
    }

    /**
     * Commits one prepared command.
     *
     * @param end the offset after the command
     */
    private void commitCommand(long end) {
        long nextCommandCount = commandCount + 1;

        putU32(COMMAND_COUNT_OFFSET, nextCommandCount);

        position = end;
        commandCount = nextCommandCount;
    }

    /**
     * Writes the HYDF header for an empty definition.
     */
    private void writeHeader() {
        buffer.set(ValueLayout.JAVA_BYTE, 0, (byte) 'H');
        buffer.set(ValueLayout.JAVA_BYTE, 1, (byte) 'Y');
        buffer.set(ValueLayout.JAVA_BYTE, 2, (byte) 'D');
        buffer.set(ValueLayout.JAVA_BYTE, 3, (byte) 'F');

        putU16(4, 1);
        putU16(6, 0);

        putU32(8, 0);
        putU32(COMMAND_COUNT_OFFSET, 0);
    }

    /**
     * Makes sure that the buffer has space for more bytes.
     *
     * @param additionalBytes the required additional size
     */
    private void ensureWritable(long additionalBytes) {
        long required = checkedEnd(position, additionalBytes);

        ensureCapacity(required);
    }

    /**
     * Makes sure that the buffer has at least the specified capacity.
     *
     * @param required the required capacity
     */
    private void ensureCapacity(long required) {
        if (required <= buffer.byteSize()) {
            return;
        }

        if (required > UINT32_MAX) {
            throw new IllegalStateException("Encoded device definition exceeds the native u32 input length limit");
        }

        long current = buffer.byteSize();

        long grown = current + (current >>> 1);

        long newCapacity = Math.clamp(grown, required, UINT32_MAX);

        Arena newArena = Arena.ofConfined();

        MemorySegment newBuffer;

        try {
            newBuffer = newArena.allocate(newCapacity, 1);

            MemorySegment.copy(buffer, 0, newBuffer, 0, position);
        } catch (RuntimeException | Error failure) {
            try {
                newArena.close();
            } catch (RuntimeException | Error closeFailure) {
                failure.addSuppressed(closeFailure);
            }

            throw failure;
        }

        Arena oldArena = Objects.requireNonNull(arena);

        arena = newArena;
        buffer = newBuffer;

        oldArena.close();
    }

    /**
     * Calculates an end offset and checks the HYDF size limit.
     *
     * @param start the start offset
     * @param length the length
     *
     * @return the end offset
     */
    private static long checkedEnd(long start, long length) {
        if (length < 0 || start > UINT32_MAX - length) {

            throw new IllegalStateException("Encoded device definition exceeds the native u32 input length limit");
        }

        return start + length;
    }

    /**
     * Checks that another HYDF command can be added.
     */
    private void requireCommandAvailable() {
        if (commandCount >= UINT32_MAX) {
            throw new IllegalStateException("HYDF command count is exhausted");
        }
    }

    /**
     * Returns the next node ID.
     *
     * @return the node ID
     */
    private int nextNodeId() {
        requireCommandAvailable();

        return (int) nodeCount;
    }

    /**
     * Returns the next parameter ID.
     *
     * @return the parameter ID
     */
    private int nextParameterId() {
        requireCommandAvailable();

        return (int) parameterCount;
    }

    /**
     * Returns the next observer ID.
     *
     * @return the observer ID
     */
    private int nextObserverId() {
        requireCommandAvailable();

        return (int) observerCount;
    }

    /**
     * Checks that an ID refers to an existing item.
     *
     * @param id the ID
     * @param count the number of existing items
     * @param name the item name for the error message
     */
    private static void requireExistingId(int id, long count, String name) {
        long unsigned = Integer.toUnsignedLong(id);

        if (unsigned >= count) {
            throw new IllegalArgumentException(name + " ID is out of range: " + unsigned);
        }
    }

    /**
     * Checks that no element is open.
     */
    private void requireTopLevel() {
        requireOpen();

        if (elementOpen) {
            throw new IllegalStateException("An element is currently being encoded");
        }
    }

    /**
     * Checks that an element is open.
     */
    private void requireElementOpen() {
        requireOpen();

        if (!elementOpen) {
            throw new IllegalStateException("No element is currently being encoded");
        }
    }

    /**
     * Checks that the current definition is complete.
     */
    private void requireComplete() {
        requireOpen();

        if (elementOpen) {
            throw new IllegalStateException("Cannot use the encoded definition while an element is unfinished");
        }
    }

    /**
     * Checks that the builder is open.
     */
    private void requireOpen() {
        if (arena == null) {
            throw new IllegalStateException("Device definition builder is closed");
        }
    }

    /**
     * Clears the state for the current element.
     */
    private void clearElementState() {
        elementOpen = false;
        elementParametersStarted = false;

        elementCommandOffset = -1;
        elementTerminalCountOffset = -1;
        elementParameterCountOffset = -1;

        elementTerminalCount = 0;
        elementParameterCount = 0;
    }

    /**
     * Writes an unsigned 16-bit value.
     *
     * @param offset the write offset
     * @param value the value
     */
    private void putU16(long offset, long value) {
        buffer.set(U16, offset, (short) value);
    }

    /**
     * Writes an unsigned 32-bit value.
     *
     * @param offset the write offset
     * @param value the value
     */
    private void putU32(long offset, long value) {
        buffer.set(U32, offset, (int) value);
    }

    /**
     * Writes the raw bits of a Java {@code int} as an unsigned 32-bit value.
     *
     * @param offset the write offset
     * @param value the value
     */
    private void putU32Raw(long offset, int value) {
        buffer.set(U32, offset, value);
    }

    /**
     * Writes one 64-bit floating-point value.
     *
     * @param offset the write offset
     * @param value the value
     */
    private void putF64(long offset, double value) {
        buffer.set(F64, offset, value);
    }

    /**
     * Releases the native memory that the builder owns.
     *
     * <p>The method has no effect when the builder is already closed.</p>
     */
    @Override
    public void close() {
        Arena ownedArena = arena;

        if (ownedArena == null) {
            return;
        }

        arena = null;
        buffer = MemorySegment.NULL;

        position = 0;

        clearElementState();

        ownedArena.close();
    }
}