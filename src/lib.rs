#![no_std]

use core::sync::atomic::{AtomicU8,Ordering};
use core::cell::Cell;

pub struct ShortQueue<const N: usize> {

    head: AtomicU8,
    tail: AtomicU8,
    buf: [Cell<u8>; N]
}

impl<const N: usize> ShortQueue<N> {

    const INIT:Cell<u8> = Cell::new(0);

    #[inline]
    fn increment( p: u8 ) -> u8 {
        p.wrapping_add(1) % (N as u8)
    }

    pub const fn new() -> Self {
        assert!( N>0 );
        assert!( N<= 256 );

        ShortQueue {
            head: AtomicU8::new(0),
            tail: AtomicU8::new(0),
            buf: [Self::INIT; N]
        }
    }

    pub const fn capacity( &self ) -> usize {
        N - 1
    }

    pub fn len( &self ) -> usize {
        let head = self.head.load( Ordering::Relaxed );
        let tail = self.tail.load( Ordering::Relaxed );

        usize::from( tail.wrapping_sub( head ).wrapping_add( N as u8 ) % (N as u8) )
    }

    #[inline]
    pub fn push( &mut self, v: u8 ) -> bool {
        self.push_inner( v )
    }

    fn push_inner( &self, v: u8 ) -> bool {
        // The tail is owned by `push`.  So the load is `Relaxed` since
        // this context's version is up to date.
        let tail = self.tail.load( Ordering::Relaxed );

        let next_tail = Self::increment( tail );

        // The queue is full if the followup write location is `head`.  The
        // load is `Acquire` because it is `Release` for the consumer 
        if next_tail == self.head.load( Ordering::Acquire ) {
            return false;
        }

        unsafe { *self.buf.get_unchecked_mut(usize::from( tail )) = v }

        // The store is `Release` so that the memory write to buf above is guaranteed
        // to be completed and broadcast to memory before `tail` is updated.
        self.tail.store( next_tail, Ordering::Release );

        true
    }

    pub fn pop( &mut self ) -> Option<u8> {
        self.pop_inner()
    }

    fn pop_inner( &self ) -> Option<u8> {

        // The head is owned by `pop`.  So the load is `Relaxed` since
        // this context's version is up to date.
        let head = self.head.load( Ordering::Relaxed );

        // The queue is empty if `head` = `tail`. The load is
        // `Acquire` since writes to `tail` by the producer are `Release`.
        if head == self.tail.load( Ordering::Acquire ) {
            return None;
        }

        let next_head = Self::increment( head );

        let rv = self.buf[ usize::from( head )].get();

        // The store is `Release` to ensure that the memory read from `buf`
        // happens before the value of `head` is updated.  Otherwise 
        // the producer might overwrite the value we are about to read.

        self.head.store( next_head, Ordering::Release );

        Some( rv )
    }

    pub fn drain( &self ) {
        self.head.store( self.tail.load(Ordering::Acquire), Ordering::Release );
    }

    pub fn is_empty( &self ) -> bool {
        self.head.load( Ordering::Relaxed ) == self.tail.load( Ordering::Relaxed )
    }

    pub fn is_full( &self ) -> bool {
        Self::increment( self.tail.load( Ordering::Relaxed) ) == self.head.load( Ordering::Relaxed )
    }

    pub fn split( &mut self ) -> (Producer<'_,N>, Consumer<'_,N>) {
        let p = Producer { core: self };
        let c = Consumer { core: self };
        ( p, c )
    }

}


pub struct Producer<'a, const N: usize> {
    core: &'a ShortQueue<N>,
}

impl<'a, const N: usize> Producer<'a, N> {

    #[inline]
    pub fn push( &mut self, b:u8 ) -> bool {
        self.core.push_inner( b )
    }

    #[inline]
    pub fn is_empty( &self ) -> bool {
        self.core.is_empty()
    }

    #[inline]
    pub fn is_full( &self ) -> bool {
        self.core.is_full()
    }
}

pub struct Consumer<'a, const N: usize> {
    core: &'a ShortQueue<N>
}

impl<'a, const N: usize> Consumer<'a, N> {

    #[inline]
    pub fn pop( &mut self ) -> Option<u8> {
        self.core.pop_inner()
    }

    #[inline]
    pub fn drain( &mut self ) {
        self.core.drain()
    }

    #[inline]
    pub fn is_empty( &self ) -> bool {
        self.core.is_empty()
    }

    #[inline]
    pub fn is_full( &self ) -> bool {
        self.core.is_full()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        let q = ShortQueue::<8>::new();

        assert_eq!( q.len(), 0 );
        assert_eq!( q.is_empty(), true );
        assert_eq!( q.is_full(), false );
        assert_eq!( q.capacity(), 7)
    }

    #[test]
    fn push() {
        let mut q = ShortQueue::<8>::new();

        for k in 0..7 {
            assert_eq!( q.push( k ), true );
        }
        assert_eq!( q.is_full(), true );

        assert_eq!( q.push( 8 ), false );
    }

    #[test]
    fn pop() {
        const QSIZE: u8 = 11;

        let mut q = ShortQueue::<{QSIZE as usize}>::new();

        for k in 0..QSIZE-1 {
            assert_eq!( q.push( k ), true );
        }

        for k in 0..QSIZE-1 {
            assert_eq!( q.pop(), Some(k) );
        }

        assert!( q.is_empty() );
        assert_eq!( q.pop(), None );
    }

    #[test]
    fn wrap() {
        const QSIZE: u8 = 6;

        let mut q = ShortQueue::< {QSIZE as usize} >::new();

        q.push(0);
        q.pop();


        for k in 1..QSIZE {
            assert_eq!( q.push( k ), true );
        }
        assert!( q.is_full() );

        for k in 1..QSIZE {
            assert_eq!( q.pop(), Some(k) );
        }

        assert!( q.is_empty() );
        assert_eq!( q.pop(), None );
    }

    #[test]
    fn drain() {
        const QSIZE:u8 = 250;

        let mut q = ShortQueue::<{QSIZE as usize}>::new();

        q.push(0);
        q.pop();


        for k in 1..QSIZE {
            assert_eq!( q.push( k ), true );
        }
        assert!( q.is_full() );

        q.drain();

        assert!( q.is_empty() );
    }

    #[test]
    fn static_new() {
        static mut _Q: ShortQueue<5> = ShortQueue::new();
    }

    #[test]
    fn split() {
        const QSIZE:u8 = 4;
        let mut q = ShortQueue::<4>::new();

        let (mut p, mut c) = q.split();

        assert!( p.push(5) );
        assert_eq!( c.pop(), Some( 5 ) );

        for k in 1..QSIZE {
            assert!( p.push(k) );
        }
        assert!( !p.push(4) );

        for k in 1..QSIZE {
            assert_eq!( c.pop(), Some(k) );
        }
    }
}
