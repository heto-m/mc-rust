package dev.mcr.bridge.net;

import io.netty.channel.AbstractEventLoop;
import io.netty.channel.Channel;
import io.netty.channel.ChannelFuture;
import io.netty.channel.ChannelPromise;
import io.netty.util.concurrent.Future;
import io.netty.util.concurrent.GlobalEventExecutor;
import io.netty.util.concurrent.ScheduledFuture;
import java.util.concurrent.Callable;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.TimeUnit;

/**
 * Confines every bridge channel to the render thread: netty hands work from any other thread to
 * {@link #execute}, and the render thread runs it in {@link #runTasks} before feeding packets.
 */
public final class RenderThreadLoop extends AbstractEventLoop {
    private static final int MAX_TASKS_PER_RUN = 1 << 16;

    private final Thread renderThread;
    private final ConcurrentLinkedQueue<Runnable> tasks = new ConcurrentLinkedQueue<>();
    private final Future<?> termination = GlobalEventExecutor.INSTANCE.newPromise();

    public RenderThreadLoop(Thread renderThread) {
        this.renderThread = renderThread;
    }

    @Override
    public boolean inEventLoop(Thread thread) {
        return thread == renderThread;
    }

    @Override
    public void execute(Runnable task) {
        tasks.add(task);
    }

    /** Render thread only; tasks queued while running wait for the next call. */
    public void runTasks() {
        for (int i = 0; i < MAX_TASKS_PER_RUN; i++) {
            Runnable task = tasks.poll();
            if (task == null) {
                return;
            }
            task.run();
        }
    }

    @Override
    public ScheduledFuture<?> schedule(Runnable command, long delay, TimeUnit unit) {
        return GlobalEventExecutor.INSTANCE.schedule(() -> execute(command), delay, unit);
    }

    @Override
    public <V> ScheduledFuture<V> schedule(Callable<V> callable, long delay, TimeUnit unit) {
        throw new UnsupportedOperationException("bridge channels schedule no callables");
    }

    @Override
    public ChannelFuture register(Channel channel) {
        throw new UnsupportedOperationException("bridge channels register on their embedded loop");
    }

    @Override
    public ChannelFuture register(ChannelPromise promise) {
        throw new UnsupportedOperationException("bridge channels register on their embedded loop");
    }

    @Deprecated
    @Override
    public ChannelFuture register(Channel channel, ChannelPromise promise) {
        throw new UnsupportedOperationException("bridge channels register on their embedded loop");
    }

    @Override
    public boolean isShuttingDown() {
        return false;
    }

    @Override
    public Future<?> shutdownGracefully(long quietPeriod, long timeout, TimeUnit unit) {
        return termination;
    }

    @Override
    public Future<?> terminationFuture() {
        return termination;
    }

    @Deprecated
    @Override
    public void shutdown() {}

    @Override
    public boolean isShutdown() {
        return false;
    }

    @Override
    public boolean isTerminated() {
        return false;
    }

    @Override
    public boolean awaitTermination(long timeout, TimeUnit unit) {
        return false;
    }
}
