// On-disk file system format.
// Both the kernel and user programs use this header file.

#define BSIZE 1024  // block size

#define NDIRECT 12
#define NINDIRECT (BSIZE / sizeof(uint))
#define MAXFILE (NDIRECT + NINDIRECT)
