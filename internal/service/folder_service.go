package service

import (
	"context"
	"fmt"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/repository"
)

// FolderService handles folder business logic.
type FolderService struct {
	folderRepo   repository.FolderRepository
	fileRepo     repository.FileRepository
	activityRepo repository.ActivityRepository
}

// NewFolderService creates a new folder service.
func NewFolderService(folderRepo repository.FolderRepository, fileRepo repository.FileRepository, activityRepo repository.ActivityRepository) *FolderService {
	return &FolderService{
		folderRepo:   folderRepo,
		fileRepo:     fileRepo,
		activityRepo: activityRepo,
	}
}

// Create creates a new folder.
func (s *FolderService) Create(ctx context.Context, folder *domain.Folder) error {
	if err := s.folderRepo.Create(ctx, folder); err != nil {
		return err
	}

	_ = s.activityRepo.Create(ctx, &domain.ActivityLog{
		UserID:     folder.OwnerID,
		Action:     domain.ActionCreate,
		TargetType: "folder",
		TargetID:   folder.ID,
		TargetName: folder.Name,
	})
	return nil
}

// GetContents returns a folder's children (folders + files).
func (s *FolderService) GetContents(ctx context.Context, folderID *string, ownerID string) (*domain.FolderContents, error) {
	var folder *domain.Folder
	if folderID != nil {
		var err error
		folder, err = s.folderRepo.GetByID(ctx, *folderID)
		if err != nil {
			return nil, err
		}
		if folder == nil {
			return nil, fmt.Errorf("folder not found")
		}
	}

	folders, err := s.folderRepo.GetChildren(ctx, folderID, ownerID)
	if err != nil {
		return nil, err
	}

	files, err := s.fileRepo.GetByFolderID(ctx, folderID, ownerID)
	if err != nil {
		return nil, err
	}

	return &domain.FolderContents{
		Folder:  folder,
		Folders: folders,
		Files:   files,
	}, nil
}

// Rename renames a folder.
func (s *FolderService) Rename(ctx context.Context, folderID, ownerID, newName string) error {
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil || folder.OwnerID != ownerID {
		return fmt.Errorf("folder not found")
	}

	folder.Name = newName
	return s.folderRepo.Update(ctx, folder)
}

// Move moves a folder to a new parent.
func (s *FolderService) Move(ctx context.Context, folderID, ownerID string, newParentID *string) error {
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil || folder.OwnerID != ownerID {
		return fmt.Errorf("folder not found")
	}

	// Prevent moving folder into its own descendant
	if newParentID != nil {
		isDesc, err := s.folderRepo.IsDescendant(ctx, folderID, *newParentID)
		if err != nil {
			return err
		}
		if isDesc {
			return fmt.Errorf("cannot move folder into its own subfolder")
		}
	}

	folder.ParentID = newParentID
	return s.folderRepo.Update(ctx, folder)
}

// Delete deletes a folder and all its contents.
func (s *FolderService) Delete(ctx context.Context, folderID, ownerID string) error {
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil || folder.OwnerID != ownerID {
		return fmt.Errorf("folder not found")
	}

	// Cascade delete handled by foreign key ON DELETE CASCADE
	if err := s.folderRepo.Delete(ctx, folderID); err != nil {
		return err
	}

	_ = s.activityRepo.Create(ctx, &domain.ActivityLog{
		UserID:     ownerID,
		Action:     domain.ActionDelete,
		TargetType: "folder",
		TargetID:   folderID,
		TargetName: folder.Name,
	})
	return nil
}

// ToggleStar toggles starred status.
func (s *FolderService) ToggleStar(ctx context.Context, folderID, ownerID string) error {
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil || folder.OwnerID != ownerID {
		return fmt.Errorf("folder not found")
	}

	folder.IsStarred = !folder.IsStarred
	return s.folderRepo.Update(ctx, folder)
}

// SetColor sets a folder's color label.
func (s *FolderService) SetColor(ctx context.Context, folderID, ownerID, color string) error {
	folder, err := s.folderRepo.GetByID(ctx, folderID)
	if err != nil {
		return err
	}
	if folder == nil || folder.OwnerID != ownerID {
		return fmt.Errorf("folder not found")
	}

	folder.Color = color
	return s.folderRepo.Update(ctx, folder)
}

// GetBreadcrumb returns the path from root to the given folder.
func (s *FolderService) GetBreadcrumb(ctx context.Context, folderID string) ([]domain.Breadcrumb, error) {
	return s.folderRepo.GetBreadcrumb(ctx, folderID)
}
