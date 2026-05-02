package service

import (
	"context"
	"fmt"
	"io"
	"log"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/repository"
	"github.com/abdullaabdullazade/freedrive/internal/storage"
)

// FileService handles file business logic.
type FileService struct {
	fileRepo     repository.FileRepository
	userRepo     repository.UserRepository
	storage      *storage.DiskStorage
	activityRepo repository.ActivityRepository
}

// NewFileService creates a new file service.
func NewFileService(fileRepo repository.FileRepository, userRepo repository.UserRepository, store *storage.DiskStorage, activityRepo repository.ActivityRepository) *FileService {
	return &FileService{
		fileRepo:     fileRepo,
		userRepo:     userRepo,
		storage:      store,
		activityRepo: activityRepo,
	}
}

// Upload stores a file and creates metadata.
func (s *FileService) Upload(ctx context.Context, file *domain.File, blobPath string) error {
	// Check quota
	user, err := s.userRepo.GetByID(ctx, file.OwnerID)
	if err != nil {
		return err
	}
	if user == nil {
		return fmt.Errorf("user not found")
	}
	if user.UsedBytes+file.EncryptedSize > user.QuotaBytes {
		return fmt.Errorf("quota exceeded: used %d + %d > %d", user.UsedBytes, file.EncryptedSize, user.QuotaBytes)
	}

	file.BlobPath = blobPath

	if err := s.fileRepo.Create(ctx, file); err != nil {
		return err
	}

	// Update user's used bytes
	if err := s.userRepo.UpdateUsedBytes(ctx, file.OwnerID, file.EncryptedSize); err != nil {
		return err
	}

	// Log activity
	s.logActivity(ctx, file.OwnerID, domain.ActionUpload, "file", file.ID, file.Name, "")

	return nil
}

// Download returns a file's blob reader.
func (s *FileService) Download(ctx context.Context, fileID, userID string) (*domain.File, func() (interface{}, error), error) {
	file, err := s.fileRepo.GetByID(ctx, fileID)
	if err != nil {
		return nil, nil, err
	}
	if file == nil {
		return nil, nil, fmt.Errorf("file not found")
	}

	// Update accessed_at
	now := time.Now()
	file.AccessedAt = now
	_ = s.fileRepo.Update(ctx, file)

	getReader := func() (interface{}, error) {
		return s.storage.Get(file.BlobPath)
	}

	return file, getReader, nil
}

// Delete moves a file to trash.
func (s *FileService) Delete(ctx context.Context, fileID, userID string) error {
	file, err := s.fileRepo.GetByID(ctx, fileID)
	if err != nil {
		return err
	}
	if file == nil {
		return fmt.Errorf("file not found")
	}
	if file.OwnerID != userID {
		return fmt.Errorf("access denied")
	}

	if err := s.fileRepo.MoveToTrash(ctx, fileID); err != nil {
		return err
	}

	s.logActivity(ctx, userID, domain.ActionDelete, "file", fileID, file.Name, "")
	return nil
}

// PermanentDelete removes a file and its blob permanently.
func (s *FileService) PermanentDelete(ctx context.Context, fileID, userID string) error {
	file, err := s.fileRepo.GetByID(ctx, fileID)
	if err != nil {
		return err
	}
	if file == nil {
		return fmt.Errorf("file not found")
	}
	if file.OwnerID != userID {
		return fmt.Errorf("access denied")
	}

	// Delete versions
	versions, _ := s.fileRepo.GetVersions(ctx, fileID)
	for _, v := range versions {
		_ = s.storage.Delete(v.BlobPath)
	}

	// Delete main blob
	if err := s.storage.Delete(file.BlobPath); err != nil {
		return err
	}

	if err := s.fileRepo.Delete(ctx, fileID); err != nil {
		return err
	}

	// Return quota
	if err := s.userRepo.UpdateUsedBytes(ctx, userID, -file.EncryptedSize); err != nil {
		return err
	}

	return nil
}

// Restore restores a file from trash.
func (s *FileService) Restore(ctx context.Context, fileID, userID string) error {
	file, err := s.fileRepo.GetByID(ctx, fileID)
	if err != nil {
		return err
	}
	if file == nil || file.OwnerID != userID {
		return fmt.Errorf("file not found")
	}

	if err := s.fileRepo.RestoreFromTrash(ctx, fileID); err != nil {
		return err
	}

	s.logActivity(ctx, userID, domain.ActionRestore, "file", fileID, file.Name, "")
	return nil
}

// Rename renames a file.
func (s *FileService) Rename(ctx context.Context, fileID, userID, newName string) error {
	file, err := s.fileRepo.GetByID(ctx, fileID)
	if err != nil {
		return err
	}
	if file == nil || file.OwnerID != userID {
		return fmt.Errorf("file not found")
	}

	oldName := file.Name
	file.Name = newName
	if err := s.fileRepo.Update(ctx, file); err != nil {
		return err
	}

	s.logActivity(ctx, userID, domain.ActionRename, "file", fileID, newName, fmt.Sprintf(`{"old_name":"%s"}`, oldName))
	return nil
}

// Move moves a file to another folder.
func (s *FileService) Move(ctx context.Context, fileID, userID string, folderID *string) error {
	file, err := s.fileRepo.GetByID(ctx, fileID)
	if err != nil {
		return err
	}
	if file == nil || file.OwnerID != userID {
		return fmt.Errorf("file not found")
	}

	file.FolderID = folderID
	if err := s.fileRepo.Update(ctx, file); err != nil {
		return err
	}

	s.logActivity(ctx, userID, domain.ActionMove, "file", fileID, file.Name, "")
	return nil
}

// ToggleStar toggles the starred state of a file.
func (s *FileService) ToggleStar(ctx context.Context, fileID, userID string) error {
	file, err := s.fileRepo.GetByID(ctx, fileID)
	if err != nil {
		return err
	}
	if file == nil || file.OwnerID != userID {
		return fmt.Errorf("file not found")
	}

	file.IsStarred = !file.IsStarred
	return s.fileRepo.Update(ctx, file)
}

// StartTrashPurge starts a background goroutine that purges old trash items.
func (s *FileService) StartTrashPurge(ctx context.Context) {
	go func() {
		ticker := time.NewTicker(1 * time.Hour)
		defer ticker.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				files, err := s.fileRepo.PurgeOldTrashed(ctx, 30)
				if err != nil {
					log.Printf("trash purge error: %v", err)
					continue
				}
				for _, f := range files {
					_ = s.storage.Delete(f.BlobPath)
					_ = s.userRepo.UpdateUsedBytes(ctx, f.OwnerID, -f.EncryptedSize)
				}
				if len(files) > 0 {
					log.Printf("purged %d old trash items", len(files))
				}
			}
		}
	}()
}

func (s *FileService) logActivity(ctx context.Context, userID string, action domain.ActivityAction, targetType, targetID, targetName, metadata string) {
	_ = s.activityRepo.Create(ctx, &domain.ActivityLog{
		UserID:     userID,
		Action:     action,
		TargetType: targetType,
		TargetID:   targetID,
		TargetName: targetName,
		Metadata:   metadata,
	})
}

// UpdateContent replaces the encrypted blob for an existing file and creates a version snapshot.
func (s *FileService) UpdateContent(ctx context.Context, fileID, userID, name, mimeType, iv string, originalSize int64, r io.Reader) (*domain.File, error) {
	file, err := s.fileRepo.GetByID(ctx, fileID)
	if err != nil {
		return nil, err
	}
	if file == nil {
		return nil, fmt.Errorf("file not found")
	}
	if file.OwnerID != userID {
		return nil, fmt.Errorf("access denied")
	}

	// Create a snapshot of current state before replacing content.
	_ = s.fileRepo.CreateVersion(ctx, &domain.FileVersion{
		FileID:    file.ID,
		Version:   file.Version,
		Size:      file.Size,
		BlobPath:  file.BlobPath,
		IV:        file.IV,
		CreatedBy: userID,
	})

	newBlobPath, newEncryptedSize, err := s.storage.Save(userID, r)
	if err != nil {
		return nil, err
	}

	user, err := s.userRepo.GetByID(ctx, userID)
	if err != nil {
		_ = s.storage.Delete(newBlobPath)
		return nil, err
	}
	if user == nil {
		_ = s.storage.Delete(newBlobPath)
		return nil, fmt.Errorf("user not found")
	}

	newUsed := user.UsedBytes - file.EncryptedSize + newEncryptedSize
	if newUsed > user.QuotaBytes {
		_ = s.storage.Delete(newBlobPath)
		return nil, fmt.Errorf("quota exceeded")
	}

	oldBlobPath := file.BlobPath
	oldEncryptedSize := file.EncryptedSize

	if name != "" {
		file.Name = name
	}
	if mimeType != "" {
		file.MimeType = mimeType
	}
	if originalSize > 0 {
		file.Size = originalSize
	}
	file.EncryptedSize = newEncryptedSize
	file.BlobPath = newBlobPath
	file.IV = iv
	file.Version += 1
	file.AccessedAt = time.Now()

	if err := s.fileRepo.Update(ctx, file); err != nil {
		_ = s.storage.Delete(newBlobPath)
		return nil, err
	}

	if err := s.userRepo.UpdateUsedBytes(ctx, userID, newEncryptedSize-oldEncryptedSize); err != nil {
		return nil, err
	}

	_ = s.storage.Delete(oldBlobPath)
	s.logActivity(ctx, userID, domain.ActionUpload, "file", file.ID, file.Name, `{"updated":true}`)

	return file, nil
}

// RestoreVersion restores a historical version as the latest file content.
func (s *FileService) RestoreVersion(ctx context.Context, fileID, userID string, version int) (*domain.File, error) {
	file, err := s.fileRepo.GetByID(ctx, fileID)
	if err != nil {
		return nil, err
	}
	if file == nil {
		return nil, fmt.Errorf("file not found")
	}
	if file.OwnerID != userID {
		return nil, fmt.Errorf("access denied")
	}

	v, err := s.fileRepo.GetVersion(ctx, fileID, version)
	if err != nil {
		return nil, err
	}
	if v == nil {
		return nil, fmt.Errorf("version not found")
	}

	_ = s.fileRepo.CreateVersion(ctx, &domain.FileVersion{
		FileID:    file.ID,
		Version:   file.Version,
		Size:      file.Size,
		BlobPath:  file.BlobPath,
		IV:        file.IV,
		CreatedBy: userID,
	})

	reader, err := s.storage.Get(v.BlobPath)
	if err != nil {
		return nil, err
	}
	defer reader.Close()

	newBlobPath, newEncryptedSize, err := s.storage.Save(userID, reader)
	if err != nil {
		return nil, err
	}

	user, err := s.userRepo.GetByID(ctx, userID)
	if err != nil {
		_ = s.storage.Delete(newBlobPath)
		return nil, err
	}
	if user == nil {
		_ = s.storage.Delete(newBlobPath)
		return nil, fmt.Errorf("user not found")
	}

	newUsed := user.UsedBytes - file.EncryptedSize + newEncryptedSize
	if newUsed > user.QuotaBytes {
		_ = s.storage.Delete(newBlobPath)
		return nil, fmt.Errorf("quota exceeded")
	}

	oldBlobPath := file.BlobPath
	oldEncryptedSize := file.EncryptedSize

	file.BlobPath = newBlobPath
	file.EncryptedSize = newEncryptedSize
	file.Size = v.Size
	file.IV = v.IV
	file.Version += 1
	file.AccessedAt = time.Now()

	if err := s.fileRepo.Update(ctx, file); err != nil {
		_ = s.storage.Delete(newBlobPath)
		return nil, err
	}

	if err := s.userRepo.UpdateUsedBytes(ctx, userID, newEncryptedSize-oldEncryptedSize); err != nil {
		return nil, err
	}

	_ = s.storage.Delete(oldBlobPath)
	s.logActivity(ctx, userID, domain.ActionRestore, "file", file.ID, file.Name, fmt.Sprintf(`{"version":%d}`, version))

	return file, nil
}
