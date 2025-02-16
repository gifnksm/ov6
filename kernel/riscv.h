#ifndef __ASSEMBLER__

static inline uint64
r_sp()
{
  uint64 x;
  asm volatile("mv %0, sp" : "=r" (x) );
  return x;
}

#endif // __ASSEMBLER__

#define PGSIZE 4096 // bytes per page
#define MAXVA (1L << (9 + 9 + 9 + 12 - 1))
