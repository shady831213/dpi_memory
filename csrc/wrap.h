#ifndef __DM_WRAP_H__
#define __DM_WRAP_H__
#include <stdint.h>
#include <stdio.h>
typedef struct{
    uint64_t base;
    uint64_t size;
} dm_mem_info ;

extern void* dm_new_allocator(const uint64_t base, const uint64_t size);
extern void* dm_new_locked_allocator(const uint64_t base, const uint64_t size);
extern uint64_t dm_alloc_addr(const void* allocator, const uint64_t size, const uint64_t align);
extern void dm_free_addr(const void* allocator, const uint64_t addr);

extern void* dm_new_space();
extern void* __dm_add_region(const void* space, char* name, void* region);
extern void __dm_clean_region(const void* space, char* name, void* ptr);
extern void* __dm_get_region(const void* space, char* name);
extern void dm_delete_region(const void* space, char* name);

extern void* dm_get_space(char* name);
void* dm_get_region(const void* space, char* name);
void* dm_add_region(const void* space, char* name, void* region);

extern void* dm_alloc_region(void* heap, uint64_t size, uint64_t align);
extern void* dm_map_region(const void* region, uint64_t base);
extern void* dm_heap(const void* region);
extern void dm_free_region(const void* region);
extern void dm_free_heap(const void* heap);
extern void* __dm_region_info(const void* region);

extern void dm_region_write_u8(const void* heap, const uint64_t addr, const uint8_t data);
extern void dm_region_write_u16(const void* heap, const uint64_t addr, const uint16_t data);
extern void dm_region_write_u32(const void* heap, const uint64_t addr, const uint32_t data);
extern void dm_region_write_u64(const void* heap, const uint64_t addr, const uint64_t data);
extern uint8_t __dm_region_read_u8(const void* heap, const uint64_t addr);
extern uint16_t __dm_region_read_u16(const void* heap, const uint64_t addr);
extern uint32_t __dm_region_read_u32(const void* heap, const uint64_t addr);
extern uint64_t __dm_region_read_u64(const void* heap, const uint64_t addr);

uint8_t dm_c_region_read_u8(const void* heap, const uint64_t addr);
uint16_t dm_c_region_read_u16(const void* heap, const uint64_t addr);
uint32_t dm_c_region_read_u32(const void* heap, const uint64_t addr);
uint64_t dm_c_region_read_u64(const void* heap, const uint64_t addr);
void dm_dpi_region_read_u8(const void* heap, const uint64_t addr, uint8_t* data);
void dm_dpi_region_read_u16(const void* heap, const uint64_t addr, uint16_t* data);
void dm_dpi_region_read_u32(const void* heap, const uint64_t addr, uint32_t* data);
void dm_dpi_region_read_u64(const void* heap, const uint64_t addr, uint64_t* data);

dm_mem_info* dm_c_region_info(const void* region);
void dm_dpi_region_info(const void* region, dm_mem_info* info);


#endif

